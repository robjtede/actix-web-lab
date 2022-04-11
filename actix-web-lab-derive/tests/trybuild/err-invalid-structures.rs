use actix_web_lab::FromRequest;

#[derive(FromRequest)]
enum Foo {
    Data(()),
}

#[derive(FromRequest)]
struct Bar(());

fn main() {}
