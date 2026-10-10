# Novatek XML parser replacement (LIBCU-863)

The Novatek read-response parsers use `quick-xml = 0.42.0` with default features
disabled. The adapter reads borrowed events from `Reader<&[u8]>`, retains twelve
optional borrowed scalar slots for the current response/file, and releases those
slots before reading another file. It does not construct a DOM, retain an event
 vector, copy XML text into strings, or build an index of the response.

The change removes the five handwritten XML/root/tag-scanning helpers and the
repeated configuration/media substring scans. Production Novatek source grows
from 1,700 to 1,766 lines because the adapter makes scalar ambiguity, accepted
structure, and streamed error precedence explicit. The maintenance benefit is
upstream tokenization plus documented protocol rules and measured throughput,
not a reduction in total source lines.

The maintained reference is
[`mame_coalesce` at 5607057](https://github.com/mjc/mame_coalesce/tree/5607057a07b1740b73199328da3815415313b39f/src).
The relevant pattern is borrowed `read_event`, bounded state for one record, and
validation of the document suffix through EOF. Its schema framework, mmap,
namespace handling, and encoding support are not required by Novatek.

## Protocol contract

The adapter remains responsible for these rules:

- Validate the response byte limit before UTF-8 or XML parsing: 4 KiB for ordinary
  responses and 512 KiB for media lists. Reject invalid UTF-8.
- Require exactly one `Function` or `LIST` root as appropriate. Permit a leading
  XML declaration and whitespace around the document. Require matching end names
  and EOF; malformed suffixes cannot produce successful evidence.
- Reject attributes, comments, CDATA, processing instructions, DTD, and a UTF-8
  BOM. Permit empty elements as before; an empty-element spelling of a required
  scalar does not supply a value.
- Preserve scalar text literally, including `&amp;`, numeric/unknown references,
  and bare ampersands. Only surrounding whitespace is trimmed. The existing URI
  and media-path validators inspect this literal evidence; entity decoding could
  turn input into different path or URI evidence.
- Ordinary responses contain direct scalar children. Ignore unknown scalar/empty
  fields. Reject nested scalar/container fields and duplicate recognized scalars
  as ambiguous XML evidence. Media accepts `LIST/File` and the observed
  `LIST/ALLFile/File` layout; nested `File` and nested `ALLFile` are rejected.
- Configuration retains ordered command/status pairing, nonzero command IDs,
  duplicate-command rejection, and the 32-pair `ArrayVec` bound. Status elements
  outside a pending pair remain ignored, preserving the prior behavior.
- Media retains the 2,048-entry cap, field widths, numeric conversion errors,
  traversal/control-character checks, and duplicate-path rejection. Retained
  entries and the uniqueness set use their existing vector/hash-set allocation
  strategy. XML reader nesting has a named 32-level safety bound; the accepted
  Novatek grammar is shallower.
- Preserve malformed-document precedence over domain errors. Once a media or
  configuration domain error occurs, retain that first error and continue XML
  validation without retaining further results. A malformed suffix wins over the
  deferred domain error.

Duplicate recognized scalar fields, nested acknowledgement fields, arbitrary
containers, and non-whitespace text between fields were accepted accidentally by
some old substring searches. The replacement explicitly rejects them. This is a
restricted camera protocol parser; general XML features are not added as part of
this replacement. Existing observed firmware, links, storage, configuration,
media, and 1,826-entry inventory fixtures remain accepted.

The eight added parser tests check ambiguous acknowledgement fields; the literal
entity/unsupported-markup policy; bare ampersands, unknown leaves, UTF-8 and BOM;
nested/duplicate media records and suffix validation; simultaneous byte/entry
limits; configuration pairing/count limits; malformed-document precedence; and unchanged media missing-field, numeric, and width errors.
The duplicate acknowledgement test was observed red on the baseline:
`Ok(Acknowledged)` instead of `Err(MalformedXml)`. The malformed-precedence test
was observed red on the first streaming implementation:
`Err(InvalidCommand)` instead of `Err(MalformedXml)`.

## Reproducing the release comparison

Run from the repository root in its pinned environment:

```console
devenv shell -- bash scripts/bench-novatek-xml.sh
```

An optional first argument selects the baseline Git revision. The default is
`6a7f7d9d54f269f94aee0d73242770f7034e2b8c`. The script copies baseline/current
Novatek modules and the standalone harness template into a temporary Cargo
project. It uses the shared `target` directory and removes only its temporary
project on exit. No legacy parser is included in production or normal test builds.
The harness pins quick-xml, arrayvec and thiserror to the versions used for this
comparison.

Each measurement includes ten warmups and nine latency samples. Each sample
runs 20,000 firmware parses, 10,000 two-file parses, or 50 inventory parses.
Reported latency is the median time per parse. Output drop is included. A separate
single parse measures successful allocation/reallocation calls, cumulative
requested bytes, and peak live parser-owned heap bytes using a forwarding global
allocator. Input generation, comparison formatting, and harness allocations are
excluded. Peak heap is not process RSS or stack usage.

Both parsers receive identical inputs. The harness compares the complete typed
output's `Debug` representation before timing and compares retained malformed-input
errors. The observed-count workload reproduces the existing fixture's fields,
filename shape and 1,826 count; it is generated metadata, not a device capture.
The maximum workload has exactly 2,048 files and is padded with allowed trailing
whitespace to exactly 524,288 bytes.

The following run used Rust 1.99.0, release optimization, on the development
machine on 2026-10-04. These are local measurements, not iOS/Android latency
claims.

| Workload | Input bytes | Baseline median | Replacement median | Baseline MiB/s | Replacement MiB/s |
|---|---:|---:|---:|---:|---:|
| Firmware | 125 | 1,049 ns | 657 ns | 113.64 | 181.44 |
| Two files | 461 | 4,092 ns | 2,052 ns | 107.44 | 214.25 |
| 1,826 files | 409,037 | 2,498,525 ns | 1,547,826 ns | 156.13 | 252.02 |
| 2,048 files | 458,765 | 2,715,754 ns | 1,657,102 ns | 161.10 | 264.02 |
| 2,048 files / 512 KiB | 524,288 | 2,829,488 ns | 1,884,408 ns | 176.71 | 265.34 |

| Workload | Baseline allocation calls / bytes / peak bytes | Replacement allocation calls / bytes / peak bytes |
|---|---:|---:|
| Firmware | 1 / 64 / 64 | 3 / 56 / 48 |
| Two files | 3 / 62,152 / 62,088 | 6 / 62,176 / 62,152 |
| Each large inventory | 13 / 3,911,152 / 2,521,104 | 16 / 3,911,176 / 2,521,168 |

The reader adds three fixed allocation calls to media parsing, 24 cumulative
requested bytes, and 64 peak bytes. The additional allocation count does not grow
with file/event count. The large retained output and path set remain the dominant
heap cost. The measured large inventories were 33–39% faster, with no event-proportional
heap growth. Latencies are development-host medians rather than isolated-device
benchmarks; repeat the script to compare on another host.

Target compilation, mobile FFI tests, Swift/iOS package/application tasks, and
Android JNI builds remain separate integration gates. These host measurements and
parser unit tests do not establish device transport acceptance.
