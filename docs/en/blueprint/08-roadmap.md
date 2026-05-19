# Chapter 8. Roadmap

This chapter defines the boundaries of Fayger's first phase and lays out evolution along known directions. Each evolution item notes its scope of impact on the current architecture so future designs can converge accordingly.

## 8.1 First-phase scope

A first-phase Fayger delivery satisfies at minimum:

- Stable contracts for the three layers (Loader / Runtime / Adapter).
- Five-part BuF artifact format with deterministic CBOR Manifest / Section_Index.
- Loading chain: Read(Header/Manifest/Index) → Parse → Verify(Structural) → Verify(Digest of Header/Manifest/Index) → Verify(Signature) → Negotiate Version → Select(Sections by LoadProfile) → Resolve → Read(Selected Section Bodies)? → HandOff.
- BuF_Source abstraction supporting any storage backend (local, network, object storage, user-defined).
- Partial loading (profile-driven section selection) and lazy loading (Eager / Lazy LoadStrategy), covering size and runtime-fetch needs of constrained terminals (drones, cameras).
- Interpreted execution path; BuF_Instance lifecycle state machine and failure isolation.
- The 8-category Universal_Instruction set and four built-in Platform_Adapters (desktop / server / browser / In-App).
- Unified error model and observability event bus; loader errors carry a `phase` tag.
- Signature verification, enforce-signature mode, and visibility for trusted-root updates.

## 8.2 First-phase non-goals

To bound first-phase complexity, the following are explicitly not delivered:

- Shared memory and IPC across BuF_Instances.
- JIT compilation and machine-code generation.
- Distributed scheduling and multi-node Fayger collaboration.
- Differential distribution of BuF and cross-artifact layered cache (partial loading already covers within-artifact section selection; cross-artifact differential reuse and a Distribution protocol are deferred).
- Built-in remote registry / Distribution protocol.

A non-goal does not mean "not needed in the future"; it means "not in the contract surface today". When later phases fill them in, they must not break existing contracts.

## 8.3 Phase 2: execution performance

Direction: add more efficient execution paths without breaking Runtime_Interface.

### 8.3.1 ExecutionStrategy abstraction

Extract "how to execute BuF semantics" as a strategy:

```idl
interface ExecutionStrategy {
  prepare(obj: BuFObject) -> PreparedExecutable
  step(exec: PreparedExecutable, ctx: ExecutionContext) -> StepResult
}
```

Phase one ships `Interpreter` as one strategy. Subsequent strategies can coexist:

- `JIT`: compile the intermediate representation in the BuF code section to machine code.
- `AOT`: produce machine code at load time (suited to release scenarios).
- `HybridTier`: interpret → probe → compile.

Strategy selection is internal to a Runtime_Implementation and does not affect Runtime_Interface's external contract.

### 8.3.2 Performance observability

New event categories:

- `ExecutionMetric`: interpret / compile / GC sampling.
- `TierTransition`: tiered execution moving from one tier to another.

Concrete strategies are not exposed externally, but callers can observe their effects.

## 8.4 Phase 3: distribution and cache

### 8.4.1 Cross-artifact layering and chunking

Phase one already supports Section-granularity on-demand reads inside a single BuF (Lazy). Subsequent phases introduce cross-artifact layered sharing:

- Resources shared across BuFs (standard libraries, model weights) become independent layers.
- Manifest gains an optional `layer_refs` declaration that references sections in other artifacts.
- A distribution protocol fetches layered resources in a shared way, and local cache hits enable cross-BuF reuse.

This requires Manifest to add optional fields, bumping `schema_version` minor accordingly. Existing BuFs are unaffected (compatibility path in §8.7).

### 8.4.2 Distribution protocol

Introduce a lightweight distribution protocol (referencing OCI Distribution Spec):

- Three-step `discover` / `pull` / `verify`.
- The distribution protocol itself is not in the Fayger process; an independent component implements it. Fayger only consumes its produced chunks.

### 8.4.3 Local cache

For BuFs already loaded and signature-verified, cache the parsed Manifest and Section results across processes. On hit, signature verification is re-run (avoiding cache poisoning).

## 8.5 Phase 4: cross-instance collaboration

### 8.5.1 Shared memory (controlled)

Allow multiple BuF_Instances within the same Fayger to share a memory region:

- Declare the `shared_memory` capability explicitly in Manifest.
- The runtime layer allocates regions centrally and exposes them per capability grants.
- Shared regions are quota-monitored; violations trigger suspension.

### 8.5.2 IPC protocol

Inter-instance messaging (within the same process or the same Fayger):

- Expressed as a new `ipc` Universal_Instruction category.
- Default-deny; Manifest must declare the peer ID set or topics.

### 8.5.3 Dependency assembly within a single Fayger

Allow a BuF to reference other BuFs as dependencies:

- The `Resolve` phase of the loading chain supports recursive resolution.
- Dependency graph expressed via a Manifest `dependencies` field.
- Cyclic dependencies are rejected at Resolve.

## 8.6 Phase 5: distributed

Direction: multi-node Fayger collaboration. Not in phase one, but reserve room in:

- BuF_Instance IDs must remain unique and addressable across nodes.
- Lifecycle event timestamps must align with external tracing systems.
- Universal_Instruction's TLV encoding already permits cross-network transport; "remote instruction dispatch" can be a separate Adapter implementation.

## 8.7 Compatibility strategy

Each evolution must follow these compatibility rules; otherwise it is a breaking change:

1. **Runtime_Interface version.** Bump per SemVer when methods are added or semantics change. BuF_Manifest expresses its minimum dependency via `runtime_interface_min`.
2. **BuF schema version.** Add optional fields → minor bump; add required fields or change existing field semantics → major bump.
3. **Error code stability.** Already-released error codes must not be reused or have their semantics changed; new ones may be added.
4. **Universal_Instruction categories.** Existing 8 categories must not have their numbers reused; new categories use reserved bits.
5. **Deprecation flow.** Mark with `deprecation_notice` first; the load-time prompt still allows it; remove only after at least one major version.

## 8.8 Tooling recommendations

To support long-term evolution, we recommend the following tooling:

- **Static dependency direction check.** Forbid the runtime layer from importing any Platform_Adapter type and from reaching into loader internals. Implementable with ArchUnit / dep-cruiser / custom lints.
- **Property-based testing baseline suite.** Capture the blueprint's correctness properties (round-trip equivalence, state-machine transitions, error chains, capability set algebra, …) as a cross-language Conformance Suite for any Runtime_Implementation to self-check.
- **Manifest CBOR validator.** Verify that Manifests use deterministic CBOR; non-deterministic encodings are non-conforming.
- **Adapter descriptor health check.** At adapter registration, immediately health-check `supported_capabilities` and `supported_instructions` for consistency (e.g., the minimum closure of capability bits versus instruction-required capabilities).

## 8.9 Phase roadmap summary

| Phase | Key directions | Primary impact | Compatibility impact |
| --- | --- | --- | --- |
| Phase 1 (current) | Three-layer contracts, interpreted execution, four built-in adapters, signatures, partial section loading, Eager/Lazy, BuF_Source abstraction | All three layers + cross-cutting | — |
| Phase 2 | ExecutionStrategy / JIT / AOT | Runtime | Compatible (additions) |
| Phase 3 | Cross-artifact layering / Distribution / local cache | Loader | Compatible (minor) |
| Phase 4 | Shared memory / IPC / dependency graph | Runtime + Loader | Compatible (additions) |
| Phase 5 | Multi-node collaboration | Adapter (remote dispatch) | Compatible (additions) |

Subsequent phase design documents will be published as standalone blueprints sharing this blueprint's terminology and contracts.
