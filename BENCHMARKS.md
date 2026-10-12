# Performance benchmarks

The CodSpeed workflow runs CPU simulation and memory profiling on pull requests and pushes to `main`. Both instruments use the same benchmark cases.

Run `just bench-test` to validate every workload once without collecting timings. Use `nix develop -c just bench-test` if local build tools are missing.

With `cargo-codspeed` installed, run `just bench-build` and `just bench-run` to check the instrumented benchmarks. Local runs do not produce CodSpeed performance measurements without the CodSpeed runner.

## Workloads

- `russe` decoder: parser startup, whole-buffer messages from 32 B to 64 KiB, fragmented messages, LF/CRLF/CR, Unicode and metadata, message batches, ignored frames, and an async stream that inserts `Pending` polls.
- `russe` encoder: event formatting, multiline Unicode, comments, retry delays, codec output into a new buffer, and codec output into a reused buffer.
- Actix SSE body: direct streams, prefilled channels, streams that insert `Pending` polls, and initial retry events. Cases vary message size and batch size.

Each workload checks expected events or bytes before measurement. Decoder and body benchmarks also reject errors while consuming the measured output.

The direct decoder cases exclude parser construction and initial buffer allocation. The startup case measures parser construction. The existing `sse_events` case includes construction, input chunk allocation, and async stream handling.

Encoder cases exclude input cloning. The new-buffer codec cases include output buffer allocation. The reused-buffer cases retain output capacity between events. Actix SSE body cases exclude input and channel setup. They measure body polling and encoding without collecting the response into another buffer.

## Interpreting results

Use CPU profiles to locate work and memory profiles to assess allocation changes. Compare all affected cases: an improvement for large messages can increase the cost of small messages or fragmented input.

CPU simulation does not measure time spent in system calls. These workloads do not establish network latency, concurrent client throughput, or backpressure behavior. Use a representative walltime workload on a dedicated runner when those properties are the optimization target.
