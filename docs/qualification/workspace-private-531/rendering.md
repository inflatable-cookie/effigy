# Closed hostagent rendering preparation — UNARMED

This is source preparation, not acceptance of the retained REFUSED observation.
Private bytes stay in their original quarantine. No private foreign path or raw
command is copied into this contract, examples or tests.

## Pinned source and option roles

Official Lima tagv2.2.1 resolves through its annotated tag to commit
`27bc4c4b101e27994e01e36566a0f2091bbca507`. [File URLs and SHA256](rendering-source.json)
pin six public source snapshots; original snapshots/tag responses are retained
in the evidence index outside Queue cleanup.

[hostagent.go](https://github.com/lima-vm/lima/blob/27bc4c4b101e27994e01e36566a0f2091bbca507/cmd/limactl/hostagent.go)
L27–115 defines the three observed option names. `--pidfile` names a PID file
(the handler reads/writes it and defers its removal); `--socket` is required and
names the hostagent's Unix HTTP API listener (the handler removes that path before
listening); `--guestagent` names an optional local guest payload passed into
hostagent creation. These are different authority roles, not interchangeable
safe strings. Their presence alone does not establish ownership or authorize any
of those upstream effects. This packet performs none of them.

[start.go](https://github.com/lima-vm/lima/blob/27bc4c4b101e27994e01e36566a0f2091bbca507/pkg/instance/start.go)
L218–288 constructs the ordered hostagent command: optional debug, hostagent,
pidfile and socket pairs, optional guestagent pair, then instance. Other upstream
branches exist (GUI, nerdctl, progress); this narrow packet deliberately refuses
them. The [filename constants](https://github.com/lima-vm/lima/blob/27bc4c4b101e27994e01e36566a0f2091bbca507/pkg/limatype/filenames/filenames.go)
L63–64 identify `ha.pid` and `ha.sock` beneath the instance directory.

[hostagent.go](https://github.com/lima-vm/lima/blob/27bc4c4b101e27994e01e36566a0f2091bbca507/pkg/hostagent/hostagent.go)
L98–110/201 passes the local guest payload toward ISO preparation;
[cidata.go](https://github.com/lima-vm/lima/blob/27bc4c4b101e27994e01e36566a0f2091bbca507/pkg/cidata/cidata.go)
L461–485 opens/decompresses it into the guest payload. The
[locator](https://github.com/lima-vm/lima/blob/27bc4c4b101e27994e01e36566a0f2091bbca507/pkg/usrlocal/usrlocal.go)
L129–145 uses OS/architecture names with an optional gzip suffix. Source explains
the option; it does not prove the retained path's contents, provenance or owner.

## Concrete v2 binding

`rendering.py` defines one closed policy: `lima-2.2.1-private-hostagent-r1`.
The owner-bound input adds exactly `rendering: {policy, comm_rule, debug, paths,
expected_atoms}`. The authenticated GO adds `rendering_sha256`, the SHA256 of
canonical JSON (`sort_keys=True`, separators comma/colon). Input and all five
program hashes remain bound. Old v1 requests refuse before target access.

`comm_rule` is exactly `reviewed-alias-full` or `reviewed-alias-prefix16`, derived
from the already-reviewed executable alias. Prefix16 describes a deterministic
expected **rendering category** motivated by this retained receipt. It is not a
universal ps/Darwin contract or executable identity witness. Exact argv0, complete
ordered atoms, PID/parent/euid/start/profile, executable/alias pins, root/owner
checks and typed path facts remain independently mandatory. There is no arbitrary
comm string or prefix length.

The derived atoms contain the alias, optional debug, hostagent, exactly the three
option/value pairs in source order and the exact private instance. Owner atoms
must equal this derivation; unknown/duplicate/reordered options, extra positionals,
credentials or foreign paths refuse even when an owner file repeats them exactly.
Pidfile and socket must be the exact instance `ha.pid`/`ha.sock`. The guestagent
must be a local Linux/aarch64 payload name under the existing control root. That
restriction is deliberately narrower than upstream, not a claim about actual
retained spelling or permission to rename/copy it.

Each role pins path, device, inode, uid, mode, kind, size and an exact parent pin
(path/device/inode/uid/mode). All paths stay under the existing control root;
symlink/noncanonical/traversal paths, unowned/writable parents, wrong types or
changed metadata refuse. A future authorized observer retains fresh lstat facts
before comparison and verifies exact parent/type/owner/identity. No content read,
PID-file modification, socket connection or trusted-root expansion is added.
The parse receipt reports only policy/category/counters and binding provenance;
original command/rendering remains private. Quarantine-before-parse, one call,
dual clocks, bounded streams and zero target signal/watch/claim surfaces remain.

## Remaining foreign-path gap

The retained receipt independently contains a socket outside the admitted roots.
This implementation keeps refusing it; neither source semantics, comm repair nor
exact ordered-atom equality can grant authority to that endpoint. The
[unarmed request](preflight-request.example.json) defines the missing next scope:
owner privately binds exactly that path and parent chain to independent provenance;
a separately approved fixed read-only request establishes canonical/type/uid/mode/
device/inode facts (or honest unavailability), without discovery, connection,
content, cleanup, adoption or follow-up. No exact foreign path appears here.
That proposal is not executable by the present observer and adds no trusted root.
Missing socket/metadata would not prove absence, health, VM closure or safe restart.

Synthetic tests use harmless analogue paths and recorded metadata, plus owned
local fixture payloads. Offline parsing is not a new native pass. Both consumed
native decisions remain consumed; any later request needs fresh explicit operator
scope/allowance, reviewed source/input/program/output/GO bindings, a new UTC and
monotonic deadline and accounting through actual report/cleanup. The future fixed
preflight invocation is unchanged; its v2 input binding is new. Administrative
signal source stays byte-identical. All qualification/consumer/final-enable and
merge holds remain.
