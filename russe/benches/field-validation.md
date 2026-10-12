# Field-validation measurements

IDs use one byte search for NUL, CR, and LF. Event names use one byte search for CR and LF. Errors still report the first invalid byte, and validation still precedes output writes.

The field-validation benchmark uses ID and event-name lengths of 6 B, 32 B, and 1 KiB. Input cloning is outside timing. Encoding and field drops are inside timing. The output buffer retains capacity.

Exploratory local CodSpeed walltime measurements on 2026-10-12, Apple M4 Max, macOS 26.5, use the best-time metric. The measurements used the checkout before the reservation follow-up in #532. CI measures the PR revision.

| ID and event-name length | Before | After | Speedup |
| ------------------------ | ------ | ----- | ------- |
| 6 B                      | 39 ns  | 34 ns | 1.15×   |
| 32 B                     | 57 ns  | 32 ns | 1.78×   |
| 1 KiB                    | 967 ns | 81 ns | 11.94×  |

[CodSpeed comparison](https://app.codspeed.io/robjtede/actix-web-lab/runs/compare/6accb46b11e2ad659290e191..6accb4c7051eb5e27d775fdf). These workload-specific gains are not additive.
