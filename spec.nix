{ lib, teamName, pname, imageName, ... }:
let
  name = pname;
  namespace = teamName;
  naisApp = {
    apiVersion = "nais.io/v1alpha1";
    kind = "Application";
    metadata = {
      inherit name namespace;
      labels.team = teamName;
      annotations = {
        "nginx.ingress.kubernetes.io/canary" = "true";
        "nginx.ingress.kubernetes.io/canary-weight" = "5";
        # V These can be tuned, for sure
        "config.linkerd.io/proxy-cpu-limit" = "4"; # Ridic number
        "config.linkerd.io/proxy-cpu-request" = "1000m";
        "config.linkerd.io/proxy-memory-request" = "256Mi";
        "config.linkerd.io/proxy-memory-limit" = "512Mi";
      };
    };
    spec = {
      ingresses = [ "https://amplitude.nav.no" ];
      image =
        "europe-north1-docker.pkg.dev/nais-management-233d/${teamName}/${imageName}";
      port = 6191;
      liveness = {
        failureThreshold = 10;
        initialDelay = 2;
        path = "/is_alive";
        periodSeconds = 10;
        port = 6969;
        timeout = 1;
      };
      prometheus = {
        enabled = true;
        path = "/metrics";
        port = "9090";
      };
      replicas = {
        min = 4;
        max = 6;
        cpuThresholdPercentage = 50;
        scalingStrategy.cpu.thresholdPercentage = 50;
      };
      accessPolicy.outbound = {
        external = [
          { host = "api.eu.amplitude.com"; }
          { host = "cdn.amplitude.com"; }
        ];
      };
      resources = {
        limits.memory = "1024Mi";
        requests = {
          cpu = "250m";
          memory = "128Mi";
        };
      };
      env = lib.attrsToList rec {
        RUST_LOG = "INFO";
        AMPLITUDE_HOST = "api.eu.amplitude.com";
        AMPLITUDE_PORT = "443";
        AMPLITUDE_SNI = AMPLITUDE_HOST;
      };
      envFrom = [{ secret = "amplitude-keys"; }];
    };
  };

  allowAllEgress = {
    apiVersion = "networking.k8s.io/v1";
    kind = "NetworkPolicy";
    metadata = {
      name = "amplitrude-proxy-eu-networkpolicy";
      inherit namespace;
    };
    spec = {
      egress = [{ to = [{ ipBlock.cidr = "0.0.0.0/0"; }]; }];
      podSelector.matchLabels.app = pname;
      policyTypes = [ "Egress" ];
    };
  };

in [ naisApp allowAllEgress ]
