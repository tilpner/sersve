sersve [![Build Status](https://travis-ci.org/tilpner/sersve.svg)](https://travis-ci.org/tilpner/sersve)
======

A simple directory server. It works for my own purposes so far, but feel free to try or contribute.

## Build

```bash
git clone https://github.com/tilpner/sersve.git
cd sersve
cargo build --release # leave off --release if impatient
target/release/sersve # target/debug/sersve respectively
# You can now visit `localhost:8080` in a browser
```

## Options

```
A minimal static file server, written in Rust with Iron.
Usage: sersve [options]
Options:
    -h, --help                  Show this message.
    -v, --version               Show the version of sersve (duh).
    -c, --config FILE           Provide a configuration file (JSON).
    -a, --address HOST          The address to bind to.
    -p, --port PORT             The port to serve.
    -r, --root ROOT             The uppermost directory to serve.
    -f, --filter REGEX          A regular expression to filter the filenames.
    -s, --size BYTES            The maximum size of a file that will be served.
    -t, --template TEMPLATE     A Mustache template to use for rendering.
    --threads THREADS           Amount of threads to use for serving.
    --fork                      Fork sersve into a background process.
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
