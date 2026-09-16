# The guide's sources

`Building Switchboard in Rust.html` is a single-page, offline guide to the
course: the five phases, one section per task with its desired behaviours,
checkpoints (each a `cargo test` filter with a tick box that persists in the
browser) and katas, and the stretch list. It is rendered by concatenating the
numbered fragments here in name order:

    exercises/guide/build.sh            # writes ~/Guides/Building Switchboard in Rust.html
    exercises/guide/build.sh out.html   # or anywhere else

`1-head.html` carries the styles and the page shell, `7-script.html` the
checkpoint script; everything between is content. The fragments are the
source of truth for the guide; the task sheets in `../tasks/` are the source
of truth for the course, and the guide must agree with them. When a sheet or
a test changes, change the matching fragment and rebuild.
