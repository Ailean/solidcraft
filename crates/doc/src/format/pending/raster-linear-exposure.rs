//! Raster product images now apply the existing saved exposure_ev control.
//! Older zero/omitted exposure retains the exact old path. Nonzero exposure
//! changes explicit rendering intent that was previously refused or ignored;
//! the last released reader asks for an update for this newer meaning.
