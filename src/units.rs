pub fn db_fs_to_linear(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

pub fn linear_to_db_fs(linear: f64) -> f64 {
    if linear < db_fs_to_linear(-80.0) {
        -80.0
    } else {
        20.0 * linear.log10()
    }
}

pub fn linear_to_db_spl(linear: f64) -> f64 {
    if linear < db_fs_to_linear(-80.0) {
        0.0
    } else {
        linear_to_db_fs(linear) + 100.0
    }
}