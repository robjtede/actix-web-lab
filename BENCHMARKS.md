# Performance benchmarks

The CodSpeed workflow runs CPU simulation and memory profiling on pull requests and pushes to `main`. Both instruments use the same benchmark cases.

Run `just bench-test` to validate every workload once without collecting timings. Use `nix develop -c just bench-test` if local build tools are missing.

With `cargo-codspeed` installed, run `just bench-build` and `just bench-run` to check the instrumented benchmarks. Local runs do not produce CodSpeed performance measurements without the CodSpeed runner.

## Workloads

- `russe` decoder: parser startup, whole-buffer messages from 32 B to 65,536 B, fragmented messages, LF/CRLF/CR, Unicode and metadata, message batches, ignored frames, and an async stream that inserts `Pending` polls.
- `russe` encoder: event formatting, multiline Unicode, comments, retry delays, codec output into a new buffer, codec output into a reused buffer, and encoded frame sizes at powers of two.
- Actix SSE body: direct streams, prefilled channels, streams that insert `Pending` polls, and initial retry events. Cases vary message size and batch size.

Each workload checks expected events or bytes before measurement. Decoder and body benchmarks also reject errors while consuming the measured output.

Message size arguments specify payload bytes. Cases include 32 B, 1 KiB, and 64 KiB, plus 1,000 B and 60,000 B. Fragment sizes also include 7 B, 13 B, 1 KiB, and 1,500 B. Batch counts include one message, 16 messages, and 128 messages.

`codec_frame_size` specifies the complete encoded frame size instead. A plain data frame adds eight bytes (`data: ` and the final blank line), so its 32 B, 1,024 B, and 65,536 B cases use payloads of 24 B, 1,016 B, and 65,528 B. Metadata adds more framing bytes in the Actix SSE cases. Use memory profiles to see which cases change allocation counts or sizes.

The direct decoder cases exclude parser construction and initial buffer allocation. The startup case measures parser construction. The existing `sse_events` case includes construction, input chunk allocation, and async stream handling.

Encoder cases exclude input cloning. The new-buffer codec cases include output buffer allocation. The reused-buffer cases retain output capacity between events. Actix SSE body cases exclude input and channel setup. They measure body polling and encoding without collecting the response into another buffer.

## Interpreting results

Use CPU profiles to locate work and memory profiles to assess allocation changes. Compare all affected cases: an improvement for large messages can increase the cost of small messages or fragmented input.

CPU simulation does not measure time spent in system calls. These workloads do not establish network latency, concurrent client throughput, or backpressure behavior. Use a representative walltime workload on a dedicated runner when those properties are the optimization target.

## Retry-formatting measurements

Retry formatting uses `itoa` on the full `u128` result of `Duration::as_millis`. It preserves `Duration::MAX`, fractional-millisecond truncation, and both retry framing forms. The existing capacity-boundary and maximum-duration tests cover buffer growth.

The reused-output benchmark covers zero, 1,500, and `u64::MAX` milliseconds. Exploratory local CodSpeed walltime measurements on the checkout before #532 use the best-time metric:

| Workload                      | Before | After | Speedup |
| ----------------------------- | ------ | ----- | ------- |
| 1,500 ms, reused codec buffer | 18 ns  | 12 ns | 1.50×   |
| 1,500 ms, ByteString output   | 27 ns  | 19 ns | 1.42×   |

[CodSpeed comparison](https://app.codspeed.io/robjtede/actix-web-lab/runs/compare/6accb46b11e2ad659290e191..6accb511f5c7e17328322e43). These measurements isolate retry formatting. The gains are specific to these workloads.
