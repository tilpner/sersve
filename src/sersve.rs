#![feature(globs, slicing_syntax)]

extern crate iron;

use std::os;
use std::path::{ Path, GenericPath };
use std::io::fs::{ File, PathExtensions };

use iron::prelude::*;
use iron::response::modifiers::*;
use iron::status;

fn text(desc: &str) -> IronResult<Response> {
    Ok(Response::new().set(Status(status::Ok)).set(Body(desc)))
}

fn serve(req: &mut Request) -> IronResult<Response> {
    let mut path = os::getcwd().ok().unwrap();
    for part in req.url.path.iter() { path.push(part[]) }
    if !path.exists() { return text("404"); } //Ok(Response::new().set(Status(status::NotFound))); }

    let content = match File::open(&path).read_to_string() {
        Ok(s) => s,
        Err(e) => return text(e.desc)
    };

    text(content[])
}

fn main() {
    Iron::new(serve).listen("127.0.0.1:80").unwrap();
}
