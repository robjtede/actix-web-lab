macro_rules! header_test_module {
    ($id:ident, $tm:ident{$($tf:item)*}) => {
        #[cfg(test)]
        mod $tm {
            #![allow(unused_imports)]

            use ::core::str;

            use ::actix_http::{Method, test};
            use ::actix_web::http::header;
            use ::mime::*;

            use crate::test::*;
            use super::{$id as HeaderField, *};

            $($tf)*
        }
    }
}

macro_rules! header_round_trip_test {
    ($id:ident, $raw:expr) => {
        #[test]
        fn $id() {
            use ::actix_http::test;

            let raw = $raw;
            let headers = raw.iter().map(|x| x.to_vec()).collect::<Vec<_>>();

            let mut req = test::TestRequest::default();

            for item in headers {
                req = req.append_header((HeaderField::name(), item)).take();
            }

            let req = req.finish();
            let value = HeaderField::parse(&req);

            let result = format!("{}", value.unwrap());
            let expected = ::std::string::String::from_utf8(raw[0].to_vec()).unwrap();

            let result_cmp: Vec<String> = result
                .to_ascii_lowercase()
                .split(' ')
                .map(|x| x.to_owned())
                .collect();
            let expected_cmp: Vec<String> = expected
                .to_ascii_lowercase()
                .split(' ')
                .map(|x| x.to_owned())
                .collect();

            assert_eq!(result_cmp.concat(), expected_cmp.concat());
        }
    };

    ($id:ident, $raw:expr, $exp:expr) => {
        #[test]
        fn $id() {
            use actix_http::test;

            let headers = $raw.iter().map(|x| x.to_vec()).collect::<Vec<_>>();
            let mut req = test::TestRequest::default();

            for item in headers {
                req.append_header((HeaderField::name(), item));
            }

            let req = req.finish();
            let val = HeaderField::parse(&req);

            let exp: ::core::option::Option<HeaderField> = $exp;

            // test parsing
            assert_eq!(val.ok(), exp);

            // test formatting
            if let Some(exp) = exp {
                let raw = &($raw)[..];
                let mut iter = raw.iter().map(|b| str::from_utf8(&b[..]).unwrap());
                let mut joined = String::new();
                if let Some(s) = iter.next() {
                    joined.push_str(s);
                    for s in iter {
                        joined.push_str(", ");
                        joined.push_str(s);
                    }
                }
                assert_eq!(format!("{}", exp), joined);
            }
        }
    };
}

pub(crate) use header_round_trip_test;
pub(crate) use header_test_module;
