use md5::{Digest, Md5};

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct Checksum(pub [u8; 16]);

impl Checksum {
    pub fn from_data(data: &[u8]) -> Self {
        let mut hasher = Md5::new();
        hasher.update(data);

        Checksum(hasher.finalize().into())
    }

    /// MD5 over the source file bytes **composed with a recipe tag**. The
    /// tag encodes the exact lossy-compression parameters used to produce
    /// the converted asset (see `CompressionLevel::recipe_fingerprint`).
    /// Two inputs with identical source bytes but different recipes
    /// produce different checksums — that's what lets `update_bank`
    /// detect stale entries when only the level assignment changed.
    pub fn from_data_with_recipe(data: &[u8], recipe: &[u8]) -> Self {
        let mut hasher = Md5::new();
        hasher.update(data);
        hasher.update(recipe);

        Checksum(hasher.finalize().into())
    }
}
