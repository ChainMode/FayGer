# Chapter 1. Introduction

## 1.1 Background

The Fay ecosystem needs an execution substrate that hides terminal and host differences and lets the same Fay artifact run consistently on desktops, servers, browsers, and embedded hosts. That substrate is **Fayger**.

By analogy:

- For Java bytecode, **Fayger is to BuF as the JVM is to class files**.
- For container images, **Fayger is to BuF as Docker / OCI Runtime is to images**.

Fayger does not prescribe how Fay's own semantics are authored. It only prescribes how a Fay artifact is loaded, verified, scheduled, and executed, and how it stays isolated from the host.

## 1.2 Positioning

Fayger is a **Virtual Runtime Environment**:

- It accepts a packaged entity called **BuF (Built-up Fay)** as input.
- It performs read, structural verification, version negotiation, signature verification, and load.
- It runs the BuF in one of several language implementations of the runtime layer.
- It translates "universal instructions" produced during execution into the target terminal's system capabilities through the adapter layer.

From the user's perspective: hand a BuF to Fayger and Fayger runs it.

## 1.3 Design principles

Fayger's first phase is designed by the following priorities:

1. **Portability first.** A BuF compiles once and runs on any adapted host. The runtime layer and adapter layer are decoupled through a stable *Universal_Instruction*.
2. **Multi-language implementations.** The runtime layer exposes a language-neutral *Runtime_Interface* that allows Rust, Go, TypeScript, and other implementations to coexist and be swapped, with no commitment to a single language ecosystem.
3. **Provable correctness.** BuF parsing and serialization satisfy round-trip equivalence, the lifecycle state machine satisfies legal-transition properties, and cross-platform behavior satisfies sequence equivalence. These properties are stated so they can be verified by property-based testing (PBT).
4. **Safe and observable by default.** Safe defaults (enforce-signature mode is available), quiet defaults (debug events off), and structured errors (unified error model with source-layer tagging and cause chains).

## 1.4 Three-layer overview

Fayger is divided vertically into three layers:

- **Loader Layer.** The boundary between BuF and the outside world. Responsible for read, write, verification, and version negotiation.
- **Runtime Layer.** The executor of BuF semantics. Supports multiple language implementations interchangeable through a unified interface contract.
- **Adapter Layer.** The boundary between Fayger and the host. Responsible for two-way translation between universal instructions and system calls.

Each layer has a clear external contract and an internal division of work. Detailed responsibilities follow in the next chapter.

## 1.5 Mapping to existing systems

| Existing system | Fayger counterpart |
| --- | --- |
| JVM ClassLoader → Verifier → ExecutionEngine | Loader Layer → Runtime Layer |
| JVM class file (magic / version / constant pool) | BuF artifact (Header / Manifest / Sections / Trailer) |
| OCI Image Manifest + Layers | BuF_Manifest + Sections |
| OCI Runtime Spec | Runtime_Interface |
| containerd vs runc | Runtime_Layer orchestration vs Runtime_Implementation execution |
| WASI capability-based security | Adapter_Layer capability negotiation |
| JNI / extism host functions | The `host` category of Universal_Instruction |

## 1.6 Glossary

- **Fayger.** The virtual runtime environment defined by this blueprint.
- **BuF (Built-up Fay).** Fay's distributable, loadable entity. The input unit of Fayger.
- **BuF_Manifest.** The metadata catalog inside a BuF (versions, entry, dependencies, capabilities, quotas, signature, …).
- **Loader_Layer / Runtime_Layer / Adapter_Layer.** Fayger's three layers.
- **Runtime_Interface.** The runtime layer's stable external contract.
- **Runtime_Implementation.** A concrete language implementation of the runtime layer.
- **Platform_Adapter.** An adapter targeting one terminal / OS in the adapter layer.
- **Universal_Instruction.** The platform-agnostic instruction passed between runtime and adapter layers.
- **BuF_Instance.** A BuF entity loaded into the runtime layer that is executable or executing.
- **Lifecycle_State.** The lifecycle state of a BuF_Instance.
- **Host_Environment.** The terminal, OS, or host application process where Fayger actually runs.
- **BuF_Source.** Storage-backend abstraction for BuF (local file, network, object storage, user-defined backends). At minimum exposes random-access read.
- **LoadProfile.** The runtime-environment profile supplied by the caller at load time. Drives Section selection. Includes target terminal class, capability set, size limits, and runtime feature flags at minimum.
- **SectionVisibility.** The visibility level of a Section: `Required` (missing means failure) or `Optional` (missing means graceful degradation).
- **LoadStrategy.** The loader's fetch strategy: `Eager` (read all selected sections up front) or `Lazy` (read header + index only; section bodies fetched on first access).

## 1.7 Blueprint organization

Subsequent chapters proceed in this order:

- Chapter 2. Overall architecture
- Chapter 3. Loader layer
- Chapter 4. Runtime layer
- Chapter 5. Adapter layer
- Chapter 6. Errors and observability
- Chapter 7. Security model
- Chapter 8. Roadmap
