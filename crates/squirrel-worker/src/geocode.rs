//! `geocode`: names the place of a geotagged blob from the offline dataset.

use std::sync::Arc;
use std::time::Duration;

use squirrel_db::media::{self, PlaceUpdate};
use squirrel_jobs::{JobContext, JobError};
use squirrel_media::geo_data::{Dataset, GeoDataError, GeoSource};
use tokio::sync::OnceCell;

use crate::context::MediaContext;
use crate::jobs::ShaPayload;

/// How long a `geocode` job waits when the dataset is not installed yet.
pub const NOT_INSTALLED_DELAY: Duration = Duration::from_secs(3600);

/// The dataset source and its index, loaded (and verified) once, on first use.
pub struct GeoCache {
    source: GeoSource,
    dataset: OnceCell<Arc<Dataset>>,
}

#[derive(Debug, thiserror::Error)]
enum LoadError {
    #[error("the geodata is not installed")]
    NotInstalled,
    #[error(transparent)]
    Data(#[from] GeoDataError),
    #[error("loading the geodata panicked: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl GeoCache {
    pub fn new(source: GeoSource) -> Self {
        Self {
            source,
            dataset: OnceCell::new(),
        }
    }

    /// No geocoding at all (tests).
    pub fn disabled() -> Self {
        Self::new(GeoSource::Disabled)
    }

    /// The id of the dataset geocoding would use now (what `media.geocoded_with`
    /// records); `None` when disabled, not installed or unusable (logged).
    /// Loads the dataset on first use.
    pub async fn installed_dataset_id(&self) -> Option<String> {
        if !self.source.is_enabled() {
            return None;
        }
        match self.dataset().await {
            Ok(dataset) => Some(dataset.id.clone()),
            Err(LoadError::NotInstalled) => None,
            Err(err) => {
                tracing::warn!(error = %err, "the geodata cannot be used");
                None
            }
        }
    }

    /// The dataset; a failed or missing load is tried again next time.
    async fn dataset(&self) -> Result<Arc<Dataset>, LoadError> {
        self.dataset
            .get_or_try_init(|| async {
                let source = self.source.clone();
                let loaded = tokio::task::spawn_blocking(move || source.load()).await??;
                let dataset = loaded.ok_or(LoadError::NotInstalled)?;
                tracing::info!(places = dataset.index.len(), id = %dataset.id, "loaded the geodata");
                Ok(Arc::new(dataset))
            })
            .await
            .cloned()
    }
}

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let ShaPayload { sha256 } = job.payload()?;
    if !ctx.geo.source.is_enabled() {
        return Ok(());
    }
    let Some(row) = media::find(&ctx.pool, &sha256)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(()); // the blob was collected meanwhile
    };
    let (Some(latitude), Some(longitude)) = (row.latitude, row.longitude) else {
        return Ok(());
    };
    let dataset = match ctx.geo.dataset().await {
        Ok(dataset) => dataset,
        Err(LoadError::NotInstalled) => return Err(JobError::Reschedule(NOT_INSTALLED_DELAY)),
        Err(err) => return Err(JobError::retry(err)),
    };
    let place = dataset
        .index
        .nearest(latitude, longitude)
        .map(|p| PlaceUpdate {
            name: &p.name,
            admin1: &p.admin1,
            country: &p.country,
            country_code: &p.country_code,
        });
    media::set_place(&ctx.pool, &sha256, place.as_ref(), &dataset.id)
        .await
        .map_err(JobError::retry)
}
