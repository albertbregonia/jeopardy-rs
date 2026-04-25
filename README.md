# jeopardy-rs
a simple, customizable, web-based implementation of the game ***Jeopardy*** written in rust

# how to run

if you wish to use all the default parameters, simply:
```bash
cargo run
```

otherwise, use env vars to specify parameters, the defaults are used here:
```bash
PORT=8080 STATIC_DIR="./src/static" cargo run
```

where
| Name | Description | Valid Input |
| - | - | - |
| `PORT` | the tcp listener port to be used by the web server (axum) | 1-65535 |
| `STATIC_DIR` | directory for the static assets of the web frontend (html/css/js) | absolute or relative path (format is OS dependent) |

# features

## current
- a
- b
- c

## upcoming
- d
- e
- f

# background