# Byte-oriented codec line measurements

Codec line processing uses `memchr2_iter` and byte slices. LF-terminated text and its LF are copied together. CR and CRLF are normalized to LF, and final empty lines are preserved. String output keeps its string-slicing algorithm to avoid an extra UTF-8 validation pass. Both paths keep the current reservation policy.

An isolated experiment on the checkout before #532 measured the following CodSpeed walltime best-time gains. That experimental checkout already had the borrowed API; CI measures this independent PR against main.

| Workload                         | Before  | After   | Speedup |
| -------------------------------- | ------- | ------- | ------- |
| 64 lines, reused codec buffer    | 399 ns  | 348 ns  | 1.15×   |
| 4,096 lines, reused codec buffer | 25.8 µs | 22.2 µs | 1.16×   |

[CodSpeed comparison](https://app.codspeed.io/robjtede/actix-web-lab/runs/compare/6accb79e051eb5e27d77602c..6accb7e1051eb5e27d77603d). The workloads contain 15 payload bytes per line and preserve the final empty line. These gains do not predict a gain for every payload shape.
