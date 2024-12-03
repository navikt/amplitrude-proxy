{ lib, teamName, pname, imageName, ... }:
let
  name = pname + "-canary";
  namespace = teamName;
  canary = {
    apiVersion = "nais.io/v1alpha1";
    kind = "Application";
    metadata = {
      inherit name namespace;
      labels.team = teamName;
      annotations = {
        "nginx.ingress.kubernetes.io/canary" = "true";
        "nginx.ingress.kubernetes.io/canary-weight" = "1";
      };
    };
    spec = {
      ingresses = [ "https://amplitude.nav.no" ];
      image =
        "europe-north1-docker.pkg.dev/nais-management-233d/team-researchops/amplitrude-proxy:v0.1.0-472-a0ece8e";
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
        min = 1;
        max = 2;
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
        limits.memory = "126";
        requests = {
          cpu = "250m";
          memory = "128Mi";
        };
      };
      env = lib.attrsToList rec {
        RUST_LOG = "TRACE";
        AMPLITUDE_HOST = "api.eu.amplitude.com";
        AMPLITUDE_PORT = "443";
        AMPLITUDE_SNI = AMPLITUDE_HOST;
      };
      envFrom = [{ secret = "amplitude-keys"; }];
    };
  };

in canary
