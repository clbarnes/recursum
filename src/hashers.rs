use std::str::FromStr;
use digest::Digest;
use digest::generic_array::ArrayLength;

pub enum HashType {
    #[cfg(feature = "meow")]
    Meow,
    #[cfg(feature = "blake2")]
    Blake2,
    #[cfg(feature = "blake3")]
    Blake3,
    #[cfg(feature = "md5")]
    Md5,
    #[cfg(feature = "sha1")]
    Sha1,
    #[cfg(feature = "sha2")]
    Sha256,
    #[cfg(feature = "sha2")]
    Sha512,
}

impl HashType {
    pub fn build(&self) -> Box<dyn Digest<OutputSize = Box<dyn ArrayLength<u8, ArrayType = >>>> {
        match self {
            #[cfg(feature = "meow")]
            Self::Meow => meowhash::MeowHasher::new(),
            #[cfg(feature = "blake2")]
            Self::Blake2 => blake2::Blake2b::new(),
            #[cfg(feature = "blake3")]
            Self::Blake3 => blake3::Hasher::new(),
            #[cfg(feature = "md5")]
            Self::Md5 => md5::Md5::new(),
            #[cfg(feature = "sha1")]
            Self::Sha1 => sha1::Sha1::new(),
            #[cfg(feature = "sha2")]
            Self::Sha256 => sha2::Sha256::new(),
            #[cfg(feature = "sha2")]
            Self::Sha512 => sha2::Sha512::new(),
        }
    }
}

impl FromStr for HashType {

}
