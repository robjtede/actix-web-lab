//! Hashing utilities for Actix Web.
//!
//! # Crate Features
//! All features are enabled by default.
//! - `blake2`: Blake2 types
//! - `blake3`: Blake3 types
//! - `md5`: MD5 types 🚩
//! - `md4`: MD4 types 🚩
//! - `sha1`: SHA-1 types 🚩
//! - `sha2`: SHA-2 types
//! - `sha3`: SHA-3 types
//!
//! # Security Warning 🚩
//! The `md4`, `md5`, and `sha1` types are included for completeness and interoperability but they
//! are considered cryptographically broken by modern standards. For security critical use cases,
//! you should move to using the other algorithms.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms, nonstandard_style)]
#![warn(future_incompatible, missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod body_hash;

pub use self::body_hash::{BodyHash, BodyHashParts};

macro_rules! body_hash_alias {
    ($name:ident, $digest:path, $feature:literal, $desc:literal, $out_size:literal) => {
        #[doc = concat!("Wraps an extractor and calculates a `", $desc, "` body checksum hash alongside.")]
        /// # Example
        ///
        /// ```
        #[doc = concat!("use actix_hash::", stringify!($name), ";")]
        ///
        #[doc = concat!("async fn handler(body: ", stringify!($name), "<String>) -> String {")]
        #[doc = concat!("    assert_eq!(body.hash().len(), ", $out_size, ");")]
        ///     body.into_parts().inner
        /// }
        /// #
        /// # // test that the documented hash size is correct
        #[doc = concat!("# type Hasher = ", stringify!($digest), ";")]
        #[doc = concat!("# const OutSize: usize = ", $out_size, ";")]
        /// # assert_eq!(
        /// #     digest::generic_array::GenericArray::<u8,
        /// #         <Hasher as digest::OutputSizeUser>::OutputSize
        /// #     >::default().len(),
        /// #     OutSize
        /// # );
        /// ```
        #[cfg(feature = $feature)]
        pub type $name<T> = BodyHash<T, $digest>;
    };
}

// Obsolete
body_hash_alias!(BodyMd4, md4::Md4, "md4", "MD4", 16);
body_hash_alias!(BodyMd5, md5::Md5, "md5", "MD5", 16);
body_hash_alias!(BodySha1, sha1::Sha1, "sha1", "SHA-1", 20);

// SHA-2
body_hash_alias!(BodySha224, sha2::Sha224, "sha2", "SHA-224", 28);
body_hash_alias!(BodySha256, sha2::Sha256, "sha2", "SHA-256", 32);
body_hash_alias!(BodySha384, sha2::Sha384, "sha2", "SHA-384", 48);
body_hash_alias!(BodySha512, sha2::Sha512, "sha2", "SHA-512", 64);

// SHA-3
body_hash_alias!(BodySha3_224, sha3::Sha3_224, "sha3", "SHA-3-224", 28);
body_hash_alias!(BodySha3_256, sha3::Sha3_256, "sha3", "SHA-3-256", 32);
body_hash_alias!(BodySha3_384, sha3::Sha3_384, "sha3", "SHA-3-384", 48);
body_hash_alias!(BodySha3_512, sha3::Sha3_512, "sha3", "SHA-3-512", 64);

// Blake2
body_hash_alias!(BodyBlake2b, blake2::Blake2b512, "blake2", "Blake2b", 64);
body_hash_alias!(BodyBlake2s, blake2::Blake2s256, "blake2", "Blake2s", 32);

// Blake3
body_hash_alias!(BodyBlake3, blake3::Hasher, "blake3", "Blake3", 32);
