//! Saves up to eight bounded linear-RGB directional lights in a Raster render job.
//! Older jobs without lights retain the exact implicit studio; an explicit empty
//! rig remains dark. No older data rewrite is needed. The last released reader
//! asks for an update instead of silently discarding this newer saved intent.
