# Guestagent public provenance and future collection — UNARMED

No installed payload, alias, root, process or runtime was inspected. All hashes
below came from bounded, inert public archive inspection under owned scratch;
no fetched code executed and no archive was extracted to the filesystem.
[payload-public-digests.json](payload-public-digests.json) pins URLs, archive hashes,
member names, byte counts and both digest domains. Original API/source snapshots
and download receipts remain in the private evidence index.

## Independent references

The official [Lima2.2.1 release](https://github.com/lima-vm/lima/releases/tag/v2.2.1)
Darwin-arm64 archive is pinned by publisher API asset digest
`9e9eacce88f37e185c346bad73aa6136f738d8cdf8c3bb23cd42b071824bc66e`.
The downloaded38,328,082-byte archive matched that hash. Its unique regular
Linux/aarch64 gzip member has7,421,099 original compressed bytes, SHA256
`ed41b146a0aa9fb5e75d4b42d7af40c9ffb2cafadf4d8685762599e1960a72d7`.
Its bounded decompressed bytes hash to
`cee7d526aa7642cad47223e8cd9a7afe7ed663188213c67ed554304e7f6f1071`.

The historical [Homebrew formula](https://github.com/Homebrew/homebrew-core/blob/0c6df12559a3519ab9ec567743faa1ea41790a6e/Formula/l/lima.rb)
pins version2.2.1 and the arm64_sequoia bottle digest
`47a100808049a4d9903d94f69c00ae74e4548fa3c3073a8aa70b847891de735b`.
It builds native payloads from source rather than installing the upstream release
archive, so byte equality must not be assumed. The38,504,444-byte bottle matched
that pinned hash. Its unique regular guestagent gzip member has7,421,321 compressed
bytes, SHA256
`ac5191e9a41bb3997c5c4d1668eaeae98c23721874158aa88a641f646ae549aa`.
Decompressed bytes hash to
`3f4949e94e44b4a83d4eb306fd69459976b8b777744d79f30ff9485e20ff0d79`.

Both compressed **and decompressed** hashes differ between these independently
version/architecture-matched public references. Outer archive SHA256, original
inner gzip SHA256, decompressed bytes and recompressed gzip are distinct domains.
Nothing was recompressed to manufacture a match. The exact installed bottle/build
provenance remains missing; no public candidate is asserted to be installed.
A future mismatch must remain refused, not trigger another observation or fetching
an arbitrary matching expected value. In-CONTROL placement remains a separate
unapproved alternative, with no copy/install/relaunch authority here.

## Fixed future evidence call

[payload_collection.py](payload_collection.py) exposes only
`collect(inputs, inputs_hash, go_index, index_hash, deadline, evidence)` through
an authenticated future owner invocation. The fixed [one-shot driver](payload_collect_once.py)
takes exactly the owner binding path and its SHA256 (see the request format),
refuses unarmed/late bindings before collection, durably consumes its own exclusive
call intent and prints only categorical results. It is a private packet entry,
not an installed product command. There is no installed CLI/env toggle.
The [complete request format](payload-collection-request.example.json) is UNARMED;
all missing operator/window/path/program/input/index/driver pins are explicit.
Matching self-authored artifacts are not owner authority.

Exact input fields are schema, source_head, programs, target_path, owner_uid,
retained_stdout and budget_seconds. The source/program pins cover common,
recovery, administrative_recovery, payload_collection, payload_collect_once and the immutable public
digest JSON. Separate GO effect `workspace-private-531-guestagent-evidence-r1`,
action `one-guestagent-metadata-hash`, count1 and allowance
`new-guestagent-evidence` require fresh Tom/operator-board/native authorization,
source/input/path-hash binding and UTC window. Old preflight/signal GOs refuse
before any target access. The target must equal the sole guestagent role in the
hash-pinned original quarantine and a closed public Lima2.2.1 locator; no free path
or substitute PID is available. Exact private spelling is not committed here.

The future scope must explicitly include that lexical path, the derived canonical
Lima2.2.1 package member and both fixed ancestor chains. Canonical mismatches,
unowned/writable parents, nonregular/symlink final payload, unsafe permissions,
oversize/non-gzip files or identity/mtime/ctime changes refuse. Metadata is written
privately before comparison; the canonical regular file is opened O_NOFOLLOW,
its held descriptor is checked against the visible inode, read at most16MiB with
deadline checks per chunk, rechecked afterward and closed on every path. This is
bounded hash collection, not extraction or execution. Parent symlink metadata
is recorded; its mode is not treated as directory permissions, and the resolved
final member must equal the closed canonical package location.

A fresh empty0700 evidence root and exclusive collection intent prevent reuse
within the same occurrence. An owner one-shot driver additionally binds this
occurrence; fresh directories do not confer retry authority. Originals and hash
facts are private0600, never raw content in public output. Reference mismatch
returns refused; a match returns `reference-match-evidence-only` with admitted,
installed-package-provenance, health and closure all false. No v2 admission input,
new trusted root, workload, signal, watch or SIGINT claim is created. The current
preflight v2 outside-root refusal is unchanged.

The owner must freeze the exact invocation and input hashes before asking for
any new native GO: <=10s dual-clock observation (driver template9s), <=600s total
including dispatch/report/cleanup, a latest-start gate with report reserve, no
retry on failure or completion, all own clients waited, immediate original-receipt
reporting. This source turn grants none of those native inputs or allowances.
All prior decisions remain consumed, original refusal retained, global budget
unchanged, production refused, aggregate UNARMED and merge held.
