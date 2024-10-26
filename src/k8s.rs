use crate::metrics::INGRESS_COUNT;
use futures::TryStreamExt;
use kube::{
	api::{Api, ListParams},
	runtime::{watcher, WatchStreamExt},
	Client,
};
use tracing::{error, info, warn};
pub mod cache;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
	group = "nais.io",
	version = "v1alpha1",
	kind = "Application",
	namespaced
)]
pub struct ApplicationT {
	pub creation_timestamp: Option<String>,
	pub ingresses: Option<Vec<String>>,
}

pub async fn populate_cache() -> Result<(), Box<dyn std::error::Error>> {
	info!("populating cache");
	let client = Client::try_default().await?;
	let app_list = Api::<Application>::all(client.clone())
		.list(&ListParams::default())
		.await?;
	for app in app_list {
		if let Some(app_info) = application_to_app_info(&app) {
			warn!("added an application: {:?}", app_info);
			app_info
				.into_iter()
				.for_each(|app| cache::insert_into_cache(app.ingress.clone(), app));
		}
	}
	let cache = cache::CACHE.lock().expect("Unable to lock cache");
	let cache_length = cache.len();
	INGRESS_COUNT.set(cache_length as f64);
	info!(
		"Cache initially populated with {} application entries",
		cache.len()
	);

	Ok(())
}

/// This watches exactly the resources in prod-gcp, where this proxy is deployed. All other clusters get a -hack- treatment (why do you have user metrics in a dev environment?)
pub async fn run_watcher() -> Result<(), Box<dyn std::error::Error>> {
	info!("Started application watcher");
	watcher(
		Api::<Application>::all(Client::try_default().await?),
		watcher::Config::default().labels("app,team"),
	)
	.applied_objects()
	.default_backoff()
	.try_for_each(move |app| async move {
		if let Some(app_info) = application_to_app_info(&app) {
			app_info.into_iter().for_each(|app| {
				INGRESS_COUNT.inc();
				cache::insert_into_cache(app.ingress.clone(), app)
			});
		}
		Ok(())
	})
	.await?;

	Ok(())
}

fn application_to_app_info(application: &Application) -> Option<Vec<cache::AppInfo>> {
	let app = application.clone();
	let ingresses = &app.spec.ingresses?;
	if ingresses.is_empty() {
		// If the app doesn't have ingress,
		// we can't map it from an `Origin` HTTP request header to an Application anyways...
		return None;
	}

	let app_name = &app
		.metadata
		.name
		.unwrap_or_else(|| "unknown app name".into());
	let namespace = &app.metadata.namespace.as_ref()?.to_owned();

	let creation_timestamp = &application
		.metadata
		.creation_timestamp
		.as_ref()?
		.0
		.to_owned();

	let app_infos: Vec<cache::AppInfo> = ingresses
		.iter()
		.map(|ingress_url| cache::AppInfo {
			app_name: app_name.to_owned(),
			namespace: namespace.into(),
			ingress: ingress_url.to_owned(),
			creation_timestamp: creation_timestamp.to_string(),
		})
		.collect();

	Some(app_infos)
}
