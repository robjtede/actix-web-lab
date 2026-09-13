//! A separate test process keeps global provider state isolated from other tests.

use std::{fs, sync::Arc};

use hot_reload_rustls::{BuildError, Builder};
use rustls::crypto::CryptoProvider;

#[test]
fn provider_selection() {
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("cert.pem");
    let key = dir.path().join("key.pem");
    let pair = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    fs::write(&cert, pair.cert.pem()).unwrap();
    fs::write(&key, pair.key_pair.serialize_pem()).unwrap();

    assert!(CryptoProvider::get_default().is_none());
    assert!(matches!(
        Builder::new(&cert, &key).build(),
        Err(BuildError::MissingProvider)
    ));
    assert!(CryptoProvider::get_default().is_none());

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .unwrap();
    let global = CryptoProvider::get_default().unwrap();
    let (config, watcher) = Builder::new(&cert, &key).build().unwrap();
    assert!(Arc::ptr_eq(config.crypto_provider(), global));
    drop(watcher);

    let explicit = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let (config, watcher) = Builder::new(&cert, &key)
        .crypto_provider(Arc::clone(&explicit))
        .build()
        .unwrap();
    assert!(Arc::ptr_eq(config.crypto_provider(), &explicit));
    assert!(!Arc::ptr_eq(config.crypto_provider(), global));
    drop(watcher);
}
