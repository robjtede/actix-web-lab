use actix_hash::{BodyHash, BodySha256};
use actix_http::BoxedPayloadStream;
use actix_web::{
    dev,
    http::StatusCode,
    test,
    web::{self, Bytes},
    App,
};
use actix_web_lab::extract::Json;
use futures_util::{stream, StreamExt as _};
use hex_literal::hex;
use sha2::{Sha256, Sha512};

#[actix_web::test]
async fn correctly_hashes_payload() {
    let app = test::init_service(
        App::new()
            .route(
                "/sha512",
                web::get().to(|body: BodyHash<Bytes, Sha512>| async move {
                    Bytes::copy_from_slice(body.hash())
                }),
            )
            .route(
                "/",
                web::get().to(|body: BodyHash<Bytes, Sha256>| async move {
                    Bytes::copy_from_slice(body.hash())
                }),
            ),
    )
    .await;

    let req = test::TestRequest::default().to_request();
    let body = test::call_and_read_body(&app, req).await;
    assert_eq!(
        body,
        hex!("e3b0c442 98fc1c14 9afbf4c8 996fb924 27ae41e4 649b934c a495991b 7852b855").as_ref()
    );

    let req = test::TestRequest::default().set_payload("abc").to_request();
    let body = test::call_and_read_body(&app, req).await;
    assert_eq!(
        body,
        hex!("ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad").as_ref()
    );

    let req = test::TestRequest::with_uri("/sha512").to_request();
    let body = test::call_and_read_body(&app, req).await;
    assert_eq!(
        body,
        hex!(
            "cf83e135 7eefb8bd f1542850 d66d8007 d620e405 0b5715dc 83f4a921 d36ce9ce
             47d0d13c 5d85f2b0 ff8318d2 877eec2f 63b931bd 47417a81 a538327a f927da3e"
        )
        .as_ref()
    );

    let (req, _) =
        test::TestRequest::default()
            .to_request()
            .replace_payload(dev::Payload::Stream {
                payload: Box::pin(
                    stream::iter([b"a", b"b", b"c"].map(|b| Bytes::from_static(b))).map(Ok),
                ) as BoxedPayloadStream,
            });

    let body = test::call_and_read_body(&app, req).await;
    assert_eq!(
        body,
        hex!("ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad").as_ref()
    );
}

#[actix_web::test]
async fn type_alias_equivalence() {
    let app = test::init_service(
        App::new()
            .route(
                "/alias",
                web::get().to(|body: BodySha256<Bytes>| async move {
                    Bytes::copy_from_slice(body.hash())
                }),
            )
            .route(
                "/expanded",
                web::get().to(|body: BodyHash<Bytes, Sha256>| async move {
                    Bytes::copy_from_slice(body.hash())
                }),
            ),
    )
    .await;

    let req = test::TestRequest::with_uri("/alias").to_request();
    let body_alias = test::call_and_read_body(&app, req).await;

    let req = test::TestRequest::with_uri("/expanded").to_request();
    let body_expanded = test::call_and_read_body(&app, req).await;

    assert_eq!(body_alias, body_expanded);
}

#[actix_web::test]
async fn respects_inner_extractor_errors() {
    let app = test::init_service(App::new().route(
        "/",
        web::get().to(|body: BodyHash<Json<u64, 4>, Sha256>| async move {
            Bytes::copy_from_slice(body.hash())
        }),
    ))
    .await;

    let req = test::TestRequest::default().set_json(1234).to_request();
    let body = test::call_and_read_body(&app, req).await;
    assert_eq!(
        body,
        hex!("03ac6742 16f3e15c 761ee1a5 e255f067 953623c8 b388b445 9e13f978 d7c846f4").as_ref()
    );

    // no body would expect a 400 content type error
    let req = test::TestRequest::default().to_request();
    let res = test::call_service(&app, req).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // body too big would expect a 413 request payload too large
    let req = test::TestRequest::default().set_json(12345).to_request();
    let res = test::call_service(&app, req).await;
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[actix_web::test]
async fn use_on_wrong_extractor() {
    let app = test::init_service(App::new().route(
        "/",
        web::get().to(
            |null: BodyHash<(), Sha256>, _body: Json<u64, 4>| async move {
                Bytes::copy_from_slice(null.hash())
            },
        ),
    ))
    .await;

    // even though the hash wrapper is not on the body extractor this should still work
    let req = test::TestRequest::default().set_json(1234).to_request();
    let res = test::call_service(&app, req).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = test::read_body(res).await;
    assert_eq!(
        body,
        hex!("03ac6742 16f3e15c 761ee1a5 e255f067 953623c8 b388b445 9e13f978 d7c846f4").as_ref()
    );
}

#[actix_web::test]
async fn use_on_wrong_extractor_in_wrong_order() {
    let app = test::init_service(App::new().route(
        "/",
        web::get().to(
            |_body: Json<u64, 4>, null: BodyHash<(), Sha256>| async move {
                Bytes::copy_from_slice(null.hash())
            },
        ),
    ))
    .await;

    let req = test::TestRequest::default().set_json(1234).to_request();
    let res = test::call_service(&app, req).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = test::read_body(res).await;
    // if the hash wrapper is on a non-body extractor _and_ a body extractor has already taken the
    // payload, this should return the empty input hash
    assert_eq!(
        body,
        hex!("e3b0c442 98fc1c14 9afbf4c8 996fb924 27ae41e4 649b934c a495991b 7852b855").as_ref()
    );
}
