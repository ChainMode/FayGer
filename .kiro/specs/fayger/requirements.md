# Requirements Document

## Introduction

Fayger 是面向 Fay 生态的虚拟运行环境（Virtual Runtime Environment），定位类似于 JVM 之于 Java 字节码、Docker 之于容器镜像。它接收一个被称为 BuF（Built-up Fay）的封装实体作为输入，对其进行加载、校验、解释并在宿主终端上执行，从而屏蔽底层操作系统、硬件架构与宿主应用之间的差异。

Fayger 在体系结构上采用三层划分：

- **加载层（Loader Layer）**：负责 BuF 的读、写、校验与版本协商，是 Fayger 与外部 BuF 制品之间的边界。
- **运行层（Runtime Layer）**：负责执行 BuF 中描述的语义。该层支持多种语言实现（Runtime Implementation），不同实现之间通过统一的运行层接口互换。
- **适配层（Adapter Layer）**：负责将运行层产生的通用指令翻译为目标终端与操作系统能够执行的系统调用，反之将系统事件归一化为通用事件。

本文档定义 Fayger 第一阶段的功能性与非功能性需求。实现细节、模块拆分、数据结构与代码组织保留至 design 阶段处理。

## Glossary

- **Fayger**：本规范定义的虚拟运行环境整体系统名称。
- **BuF（Built-up Fay）**：Fay 的可分发、可加载实体，是 Fayger 的输入单元，由元数据、内容段与校验信息组成。
- **BuF_Manifest**：BuF 内部用于描述自身元数据（版本、入口、依赖、签名摘要等）的清单。
- **Loader_Layer**：加载层，Fayger 中负责 BuF 读写与校验的子系统。
- **BuF_Parser**：将 BuF 字节流解析为内存中 BuF 对象的组件，隶属于 Loader_Layer。
- **BuF_Serializer**：将内存中的 BuF 对象序列化回 BuF 字节流的组件（即 Pretty Printer 角色），隶属于 Loader_Layer。
- **Runtime_Layer**：运行层，Fayger 中负责执行 BuF 语义的子系统。
- **Runtime_Implementation**：运行层的一种具体语言实现（例如 Rust 实现、Go 实现、TypeScript 实现），实现统一的 Runtime_Interface。
- **Runtime_Interface**：运行层对外暴露的稳定接口契约，所有 Runtime_Implementation 必须遵循。
- **Adapter_Layer**：适配层，Fayger 中负责把通用指令翻译为目标平台系统调用的子系统。
- **Platform_Adapter**：Adapter_Layer 中针对某一特定终端/操作系统的具体适配器（例如 Linux_Adapter、Windows_Adapter、Browser_Adapter、InApp_Adapter）。
- **Universal_Instruction**：Runtime_Layer 与 Adapter_Layer 之间约定的、与平台无关的指令集。
- **Host_Environment**：Fayger 实际运行所在的终端、操作系统或宿主应用进程。
- **BuF_Instance**：一个被加载到 Runtime_Layer 中、处于可执行或执行中状态的 BuF 运行时实体。
- **Lifecycle_State**：BuF_Instance 的生命周期状态集合，至少包含 `Loaded`、`Initialized`、`Running`、`Suspended`、`Terminated`、`Failed`。
- **BuF_Source**：BuF 的存储后端抽象，可以是本地文件、网络流、对象存储或任意用户自定义存储。Loader_Layer 通过 BuF_Source 读取 BuF，最低要求 BuF_Source 暴露随机访问读能力。
- **LoadProfile**：加载时由调用方提供的运行环境画像，用于驱动 Section 选择，至少包含目标终端类别、可用能力集（capabilities）、体积上限与可选的运行时特征位（required_features）。
- **SectionVisibility**：Section 在 BuF_Manifest 中的可见性等级枚举，至少包含两种取值：`Required`（必需 Section，缺失即视为加载失败）、`Optional`（可选 Section，profile 不满足时跳过并降级）。
- **LoadStrategy**：加载层的取数策略枚举，至少包含两种取值：`Eager`（在 load 调用返回前完成所有被选中 Section 的读取）、`Lazy`（load 调用返回前仅完成 Header / Manifest / Section_Index 的读取，被选中 Section 体在被首次访问时才从 BuF_Source 取回）。

## Requirements

### Requirement 1：BuF 加载与校验

**User Story:** 作为 Fay 应用的发布者，我希望将 BuF 直接交给 Fayger，Fayger 能够读取并校验它，以便我无需关心目标终端差异。

#### Acceptance Criteria

1. WHEN 用户向 Fayger 提交一个 BuF 字节流，THE Loader_Layer SHALL 在不执行 BuF 内容的前提下完成结构性解析。
2. WHEN BuF_Parser 解析 BuF 字节流，THE BuF_Parser SHALL 产出一个包含 BuF_Manifest 与内容段的 BuF 内存对象。
3. WHEN BuF_Manifest 中声明的 schema 版本不被当前 Fayger 支持，THE Loader_Layer SHALL 拒绝加载并返回包含期望版本范围的错误。
4. IF BuF 字节流不符合 BuF 格式规范，THEN THE Loader_Layer SHALL 终止加载并返回包含失败位置与原因的错误。
5. IF BuF_Manifest 中的内容摘要与实际内容段计算结果不一致，THEN THE Loader_Layer SHALL 终止加载并返回校验失败错误。

### Requirement 2：BuF 序列化与回写

**User Story:** 作为 Fay 工具链的开发者，我希望 Fayger 同时具备 BuF 的写入能力，以便对 BuF 进行修改、重打包或缓存。

#### Acceptance Criteria

1. THE Loader_Layer SHALL 提供 BuF_Serializer，用于把内存中的 BuF 对象写出为 BuF 字节流。
2. WHEN BuF_Serializer 处理一个由 BuF_Parser 产出的 BuF 对象，THE BuF_Serializer SHALL 输出一个符合 BuF 格式规范的字节流。
3. WHEN 对任意合法的 BuF 字节流先解析再序列化得到的字节流再次解析，THE Loader_Layer SHALL 产出与首次解析在结构上等价的 BuF 对象（往返一致性属性）。
4. IF 待序列化的 BuF 对象缺少 BuF_Manifest 必填字段，THEN THE BuF_Serializer SHALL 拒绝序列化并返回缺失字段列表。

### Requirement 3：Runtime_Interface 统一契约

**User Story:** 作为运行层实现者，我希望存在一份与具体语言无关的运行层接口契约，以便我可以使用任意语言提供 Runtime_Implementation。

#### Acceptance Criteria

1. THE Runtime_Layer SHALL 定义一份 Runtime_Interface，覆盖 BuF_Instance 的加载、初始化、执行、暂停、恢复与终止操作。
2. THE Runtime_Interface SHALL 通过 Universal_Instruction 与 Adapter_Layer 通信，且不引用任何 Platform_Adapter 的细节。
3. WHEN Fayger 被请求注册一个 Runtime_Implementation，THE Runtime_Layer SHALL 校验其是否实现 Runtime_Interface 全部必需操作。
4. IF Runtime_Implementation 缺少 Runtime_Interface 任一必需操作，THEN THE Runtime_Layer SHALL 拒绝注册并返回缺失操作列表。
5. WHERE 同一 Fayger 实例同时存在多个已注册的 Runtime_Implementation，THE Runtime_Layer SHALL 按照 BuF_Manifest 中声明的运行层选择策略路由 BuF_Instance。

### Requirement 4：BuF 执行与生命周期

**User Story:** 作为 Fay 应用的使用者，我希望 BuF 在 Fayger 中具有可观察、可控制的运行生命周期，以便我能够启动、暂停或停止它。

#### Acceptance Criteria

1. WHEN Loader_Layer 成功完成 BuF 加载，THE Runtime_Layer SHALL 创建一个处于 `Loaded` 状态的 BuF_Instance。
2. WHEN BuF_Instance 收到初始化指令，THE Runtime_Layer SHALL 将其状态由 `Loaded` 迁移至 `Initialized`。
3. WHEN BuF_Instance 收到启动指令且其状态为 `Initialized` 或 `Suspended`，THE Runtime_Layer SHALL 将其状态迁移至 `Running` 并开始执行 BuF_Manifest 中声明的入口。
4. WHEN BuF_Instance 收到暂停指令且其状态为 `Running`，THE Runtime_Layer SHALL 将其状态迁移至 `Suspended` 并停止派发新的 Universal_Instruction。
5. WHEN BuF_Instance 收到终止指令，THE Runtime_Layer SHALL 将其状态迁移至 `Terminated` 并释放占用的运行层资源。
6. IF BuF_Instance 在执行过程中抛出未被处理的错误，THEN THE Runtime_Layer SHALL 将其状态迁移至 `Failed` 并保留错误上下文供查询。
7. IF BuF_Instance 收到与当前 Lifecycle_State 不兼容的状态迁移指令，THEN THE Runtime_Layer SHALL 拒绝该指令并返回当前状态与允许的迁移集合。

### Requirement 5：Universal_Instruction 与适配层翻译

**User Story:** 作为 Fayger 的可移植性维护者，我希望运行层产生的指令经过适配层翻译后才接触宿主，以便新增终端无需改动运行层。

#### Acceptance Criteria

1. THE Adapter_Layer SHALL 接收 Runtime_Layer 产生的 Universal_Instruction 并将其翻译为目标 Host_Environment 的系统调用。
2. THE Adapter_Layer SHALL 将 Host_Environment 产生的系统事件归一化为 Universal_Instruction 回送给 Runtime_Layer。
3. WHEN Fayger 启动时，THE Adapter_Layer SHALL 自动选取与当前 Host_Environment 匹配的 Platform_Adapter。
4. IF 当前 Host_Environment 没有匹配的 Platform_Adapter，THEN THE Adapter_Layer SHALL 阻止 Fayger 进入可服务状态并返回不支持的 Host_Environment 描述。
5. IF 一条 Universal_Instruction 在当前 Platform_Adapter 中没有对应实现，THEN THE Adapter_Layer SHALL 返回 `UnsupportedInstruction` 错误并附带指令标识。

### Requirement 6：多终端与宿主形态适配

**User Story:** 作为跨平台用户，我希望同一份 BuF 在桌面、服务器、浏览器与嵌入式宿主应用中行为一致，以便我只需打包一次。

#### Acceptance Criteria

1. THE Adapter_Layer SHALL 至少支持四类 Host_Environment：原生桌面操作系统、服务器操作系统、浏览器环境、宿主 In-App 嵌入环境。
2. WHEN 同一个 BuF 在不同 Platform_Adapter 上被加载并执行，THE Runtime_Layer SHALL 在 Lifecycle_State 序列与 Universal_Instruction 序列上保持一致。
3. WHERE Host_Environment 提供受限的资源访问能力（例如浏览器沙箱），THE Adapter_Layer SHALL 在 BuF_Manifest 声明的能力集与实际可用能力集之间执行交集裁剪并向 Runtime_Layer 报告差异。
4. IF BuF_Manifest 声明的能力在当前 Host_Environment 中完全不可用，THEN THE Adapter_Layer SHALL 拒绝进入 `Running` 状态并返回缺失能力列表。

### Requirement 7：资源隔离与配额

**User Story:** 作为宿主管理员，我希望多个 BuF_Instance 之间互不干扰，并且每个实例的资源占用可控，以便保证宿主稳定。

#### Acceptance Criteria

1. THE Runtime_Layer SHALL 为每一个 BuF_Instance 维护独立的运行时数据区，且不同 BuF_Instance 之间默认不共享内存。
2. WHERE BuF_Manifest 声明了 CPU、内存或 I/O 配额，THE Runtime_Layer SHALL 在执行期间对 BuF_Instance 的实际使用量进行监测。
3. IF BuF_Instance 的资源使用量超出 BuF_Manifest 中声明的配额，THEN THE Runtime_Layer SHALL 暂停该 BuF_Instance 并返回超限的资源类型与当前用量。
4. IF 某一 BuF_Instance 进入 `Failed` 状态，THEN THE Runtime_Layer SHALL 保证其他处于 `Running` 状态的 BuF_Instance 不被该失败影响。

### Requirement 8：错误模型与诊断

**User Story:** 作为 Fay 开发者，我希望 Fayger 返回结构化、可定位的错误，以便我能快速排查问题。

#### Acceptance Criteria

1. THE Fayger SHALL 对所有面向用户返回的错误使用统一的错误模型，至少包含错误码、所属层、可读描述与上下文键值对。
2. WHEN Loader_Layer、Runtime_Layer 或 Adapter_Layer 产生错误，THE Fayger SHALL 在错误对象中标注该错误的来源层。
3. WHEN BuF_Instance 进入 `Failed` 状态，THE Runtime_Layer SHALL 暴露查询接口，返回触发失败的最末一条 Universal_Instruction 与对应错误对象。
4. IF 同一次操作在多层连续产生错误，THEN THE Fayger SHALL 保留错误链而不是仅返回最末错误。

### Requirement 9：可观测性

**User Story:** 作为运维人员，我希望可以观察 Fayger 与 BuF_Instance 的运行状况，以便监控与定位问题。

#### Acceptance Criteria

1. THE Fayger SHALL 暴露 BuF_Instance 当前 Lifecycle_State 与已注册 Runtime_Implementation 列表的查询接口。
2. WHEN BuF_Instance 发生 Lifecycle_State 迁移，THE Runtime_Layer SHALL 发出携带迁移前状态、迁移后状态与时间戳的事件。
3. WHERE Host_Environment 提供日志或追踪通道，THE Adapter_Layer SHALL 把 Fayger 内部事件映射到该通道。
4. THE Fayger SHALL 提供启用与禁用调试级事件输出的开关，且默认处于禁用状态。

### Requirement 10：版本协商与兼容性

**User Story:** 作为长期维护者，我希望 BuF 与 Fayger 之间的版本兼容关系清晰，以便老版本 BuF 仍能在新版本 Fayger 中运行。

#### Acceptance Criteria

1. THE BuF_Manifest SHALL 声明其遵循的 BuF schema 版本与所需 Runtime_Interface 最低版本。
2. WHEN BuF_Manifest 声明的 Runtime_Interface 最低版本不高于当前 Fayger 提供的 Runtime_Interface 版本，THE Runtime_Layer SHALL 允许该 BuF 加载。
3. IF BuF_Manifest 声明的 Runtime_Interface 最低版本高于当前 Fayger 提供的版本，THEN THE Runtime_Layer SHALL 拒绝加载并返回所需版本与当前版本。
4. WHERE BuF schema 版本在当前 Fayger 中标记为弃用但仍受支持，THE Loader_Layer SHALL 允许加载并返回弃用提示。

### Requirement 11：安全与签名校验

**User Story:** 作为分发方，我希望 Fayger 能够校验 BuF 的来源与完整性，以便防止被篡改的 BuF 被执行。

#### Acceptance Criteria

1. WHERE BuF_Manifest 中包含签名信息，THE Loader_Layer SHALL 在加载阶段使用 BuF_Manifest 声明的算法验证签名。
2. IF BuF 签名验证失败，THEN THE Loader_Layer SHALL 拒绝加载并返回签名验证失败错误。
3. WHERE Fayger 配置为强制签名模式，THE Loader_Layer SHALL 拒绝加载任何未携带签名的 BuF。
4. WHEN 校验签名所需的可信根集合发生变更，THE Loader_Layer SHALL 在下一次加载时使用更新后的可信根集合。

### Requirement 12：按运行环境的部分加载（Profile-Driven Partial Loading）

**User Story:** 作为性能受限终端（无人机、摄像头等）上的 Fayger 集成方，我希望同一个 BuF 在加载时仅读取与当前运行环境相关的 Section 子集，以便在保留 BuF 单一来源的同时适配不同终端的资源约束。

#### Acceptance Criteria

1. WHEN Loader_Layer 接收 BuF 时调用方携带 LoadProfile，THE Loader_Layer SHALL 仅加载在该 LoadProfile 下被选中的 Section 集合，且未被选中的 Section 不被读取至运行时数据区。
2. THE BuF_Manifest SHALL 允许每个 SectionDescriptor 携带 profile 约束字段，至少包含 `required_capabilities`、`max_size`、`required_features` 与 SectionVisibility。
3. WHEN 当前 Host_Environment 与 LoadProfile 的组合不满足某 Section 的 profile 约束且该 Section 的 SectionVisibility 取值为 `Required`，THE Loader_Layer SHALL 拒绝加载该 BuF 并在错误中返回缺失原因，包含 Section_Id 与未满足的约束项列表。
4. WHEN 当前 Host_Environment 与 LoadProfile 的组合不满足某 Section 的 profile 约束且该 Section 的 SectionVisibility 取值为 `Optional`，THE Loader_Layer SHALL 跳过该 Section 并在加载结果中记录被跳过的 Section_Id 列表与每个 Section 被跳过的原因。
5. THE Loader_Layer SHALL 保证部分加载完成后的 BuF_Instance 满足 BuF_Manifest 整体签名校验通过，且每一个被实际加载的 Section 满足自身在 BuF_Manifest 中声明的 digest 校验。

### Requirement 13：多源存储与延迟读取（Multi-Source Storage & Lazy Loading）

**User Story:** 作为 BuF 的发布方，我希望把 BuF 托管在任意存储后端，并允许 Fayger 在加载时按需选择一次性加载或运行时按需读取，以便在带宽与启动时延之间做权衡。

#### Acceptance Criteria

1. THE Loader_Layer SHALL 通过 BuF_Source 抽象读取 BuF，且 BuF_Source 至少暴露随机访问读能力。
2. WHEN 加载策略 LoadStrategy 取值为 `Eager`，THE Loader_Layer SHALL 在 load 调用返回前完成所有被选中 Section 的读取与 digest 校验。
3. WHEN 加载策略 LoadStrategy 取值为 `Lazy`，THE Loader_Layer SHALL 在 load 调用返回前仅完成 Header、Manifest 与 Section_Index 的读取与校验，并保留 BuF_Source 引用以供后续按需读取。
4. WHEN Runtime_Layer 在 LoadStrategy 为 `Lazy` 的 BuF_Instance 上首次访问某个被选中但尚未读取的 Section，THE Loader_Layer SHALL 通过 BuF_Source 读取该 Section 并对其执行 BuF_Manifest 中声明的 digest 校验。
5. IF LoadStrategy 为 `Lazy` 时 BuF_Source 在后续访问中不可用或读取失败，THEN THE Loader_Layer SHALL 返回结构化错误，错误上下文中包含失败的 Section_Id 与底层错误对象。
6. THE Loader_Layer SHALL 在 Fayger 统一错误模型中区分 Eager 加载阶段失败与 Lazy 后续访问阶段失败，使两类失败可被调用方按错误码或上下文字段独立处理。
