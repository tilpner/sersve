sersve [![Build Status](https://travis-ci.org/hoeppnertill/sersve.svg)](https://travis-ci.org/hoeppnertill/sersve)
======

A simple directory server. It works for my own purposes so far, but feel free to try or contribute.

[Relevant blog article.](http://till.hoeppner.ws/2014/11/30/Introducing-sersve-a-directory-server-in-Rust-with-Iron/)

## Build

```bash
git clone https://github.com/hoeppnertill/sersve.git
cd sersve
cargo build --release # leave off --release if impatient
target/release/sersve # target/sersve respectively
# You can now visit `localhost:8080` in a browser
```

## Options

```
Usage: target/release/sersve [options]
A minimal directory server, written in Rust with Iron.
Options:
    -c --config NAME    set config file name
    -a --address HOST   the address to bind to
    -p --port PORT      the port to serve
    -r --root ROOT      the uppermost directory to serve
    -f --filter REGEX   a regular expression to filter the filenames
    -s --size BYTES     the maximum size of a file that will be served
    -t --template TEMPLATE
                        a mustache template to use for rendering
    -h --help           print this help menu
```

## Configuration format

```json
{
    "address": "0.0.0.0",
    "port": 8080,
    "root": "/home/",
    "filter": "^[^\\.]+$",
    "template": "<!DOCTYPE html><html><title>{{title}}</title><body><div id=\"container\"><h1>{{title}}</h1><table><thead><tr><th>Name</th><th>Size</th></tr></thead><tbody>{{#content}} <tr> <td> <a href=\"/{{url}}\">{{name}}</a> </td> <td> {{size}} </td> </tr> {{/content}} </tbody> </table> </div> </body></html>",
    "size": 10485760
}
```
