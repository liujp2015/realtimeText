# **基于 Tauri v2 与阶跃星辰端到端大模型的全局实时语音捕获与字幕渲染系统架构研究**

## **行业背景与系统愿景**

在现代人机交互与多媒体分析领域，能够全局捕获操作系统音频并实时转化为字幕的桌面应用程序（如 Wispr Flow）正逐渐成为提高生产力的关键工具。此类系统突破了传统软件仅能处理麦克风输入的局限，将视野扩展至系统全局音频的回环捕获（Loopback Capture），从而能够为在线会议、播客播放、流媒体视频等任何发出声音的应用程序提供实时的无障碍字幕覆盖与结构化内容存档。  
构建此类系统的核心技术挑战在于跨平台的底层音频捕获、极低延迟的数字信号处理、高效的流式网络通信以及无缝的桌面图形渲染。在传统的级联语音处理管线中，音频数据的捕获、语音活动检测（VAD）、自动语音识别（ASR）以及自然语言处理（NLP）通常被划分为多个独立的服务模块。这种架构往往会导致累积的端到端延迟，无法满足实时字幕严苛的同步要求。  
为解决上述挑战，新一代系统架构转向使用 Tauri v2 框架结合阶跃星辰（Stepfun）的端到端实时语音大模型。Tauri v2 凭借 Rust 语言出色的内存安全性与接近 C/C++ 的底层执行效率，为高吞吐量的音频数字信号处理与系统级进程间通信（IPC）提供了坚实的底座，同时其基于系统原生 Webview 的轻量级前端渲染机制，能够以极低的资源占用实现沉浸式的悬浮字幕界面。阶跃星辰推出的 StepAudio 2.5 Realtime 端到端大模型，则通过单一的 WebSocket 流式接口将理解与生成融为一体，彻底消除了级联架构的中间延迟。本报告将对如何从零开始设计并实现这一复杂的异构系统进行深度的剖析。

## **跨平台全局系统音频回环捕获架构**

全局音频捕获的技术本质是拦截操作系统音频子系统在混音后发送至物理扬声器的数字信号。由于桌面端操作系统（Windows、macOS、Linux）在多媒体框架上的历史演进与安全隔离机制存在极大的差异，实现一套稳定、统一的回环捕获接口是系统开发的首要难题。

### **操作系统底层音频接口差异分析**

在 Windows 平台上，音频架构相对开放。Windows Audio Session API (WASAPI) 原生提供了对回环捕获的支持。开发者只需在初始化输出设备流时，传入特定的标志位（如 AUDCLNT\_STREAMFLAGS\_LOOPBACK），系统便会创建一个镜像的输入流，将扬声器的音频数据精准地回传给应用程序1。在早期的 Python 原型开发中，研究人员通常需要依赖经过特殊补丁修改的库（如 pyaudiowpatch）才能调用 WASAPI 的回环功能3。而在 Rust 生态中，基于 WASAPI 构建的跨平台库提供了更为原生的支持。  
与 Windows 形成鲜明对比的是 macOS 系统。出于严格的用户隐私保护与版权数字版权管理（DRM）考量，苹果长期以来在 CoreAudio 框架中屏蔽了直接捕获系统全局输出的接口。在传统的解决方案中，开发者必须指导用户安装内核级的虚拟音频驱动程序，如 Soundflower、BlackHole（支持 2 通道、16 通道甚至 64 通道）或商业软件 Rogue Amoeba 的 Loopback6。这些虚拟设备在操作系统中伪装成扬声器接收音频，同时伪装成麦克风将音频输送给捕获软件。然而，这种方案增加了极高的分发与安装摩擦力。随着 macOS 14 的发布，苹果引入了 ScreenCaptureKit 框架，允许应用程序在获取屏幕录制权限后，直接在应用层捕获系统音频。最新的 Rust 音频库正在逐步整合这一现代 API，以实现免驱动的 Mac 全局音频捕获9。  
Linux 环境下的音频子系统则呈现出碎片化的特征。无论底层使用的是 ALSA，还是上层基于 PulseAudio 或新兴的 PipeWire 声音服务器，其核心逻辑均围绕“源”（Source）与“接收器”（Sink）展开。在 PulseAudio 与 PipeWire 架构中，每一个负责输出的 Sink 都会在内核空间自动映射出一个对应的 .monitor 接口。应用程序只需将读取流绑定至该 Monitor 接口，即可无损获取全局音频数据11。

### **基于 Rust 跨平台音频库的流式捕获实现**

为了在 Tauri v2 的后端抹平上述底层差异，系统采用 Rust 社区维护的 cpal（Cross-Platform Audio Library）作为统一的音频硬件抽象层12。cpal 将各种复杂的系统调用封装为了统一的主机（Host）、设备（Device）与流（Stream）概念。  
在初始化阶段，系统首先实例化默认的音频主机，并获取当前的默认输出设备，而非输入设备13。这一步骤是回环捕获的基石。随后，系统需枚举该输出设备支持的所有音频格式与采样率配置。通常情况下，现代操作系统默认的输出格式为 44.1kHz 或 48kHz 的采样率、立体声双声道（Stereo）以及 32 位浮点数（f32）编码13。  
获取到设备配置后，系统需调用特定 API 构建输入流（尽管物理上这是一个输出设备，但在回环模式下，程序是以“输入”的身份读取数据）15。音频流一旦启动，操作系统底层的音频守护进程将会以极高的频率（通常几毫秒一次）调用应用程序注册的回调函数。  
音频回调函数运行在一个具有实时调度优先级（Real-time priority）的特殊线程中。在这一线程内，严禁执行任何可能引起阻塞的操作，包括内存分配、互斥锁（Mutex）争用、磁盘写入或网络 I/O。任何微小的阻塞都会导致音频缓冲区未能及时清空，引发数据溢出（Buffer Overrun），最终在用户端表现为声音的爆音或卡顿。因此，系统必须预先分配一段无锁的环形缓冲区（Lock-free Ring Buffer，例如 rtrb 库中的堆分配环形缓冲区）。音频回调函数唯一的工作就是将系统传入的原始字节流安全地压入环形缓冲区，随后立即返回，将繁重的数字信号处理交由另一个独立的后台工作线程完成15。

## **高频数字信号处理与重采样管道**

阶跃星辰（Stepfun）的实时流式语音识别 API 具备极高的识别精度与响应速度，但前提是输入端必须严格遵守其设定的音频格式规范。根据 API 文档，该服务要求客户端通过 WebSocket 实时推送的音频必须为单声道（Mono）、16000 Hz 采样率、采用 16 位小端有符号整数（pcm\_s16le）编码的原始 PCM 数据16。这与操作系统默认输出的 48000 Hz、双声道、32 位浮点数据存在巨大差异，必须构建一个实时且极低延迟的数字信号处理（DSP）管道进行格式转换。

### **通道缩减与下采样理论**

音频转换的第一个关键步骤是立体声到单声道的混音（Downmixing）。操作系统捕获的浮点数组通常是交错排列的（Interleaved），即奇数索引代表左声道，偶数索引代表右声道。工作线程从环形缓冲区拉取到数据后，通过对相邻的左右声道样本进行算术平均，能够将双通道信号无损地合并为单一通道信号14。  
紧接着的下采样（Downsampling）过程是整个 DSP 管道中最具挑战性的环节。将 48000 Hz 的信号降低至 16000 Hz，意味着每三个样本中只能保留一个。然而，根据奈奎斯特-香农采样定理（Nyquist–Shannon sampling theorem），如果原始信号中存在高于 8000 Hz（目标采样率 16000 Hz 的一半）的高频分量，直接丢弃样本将导致严重的高频信号混叠（Aliasing），产生刺耳的低频伪影噪音18。为了避免混叠，系统必须在下采样之前，对信号应用抗混叠低通滤波器（通常是有限脉冲响应 FIR 滤波器）。在 Rust 生态中，集成如 rubato 这样的专业重采样库，可以在保证相位线性的前提下高效完成低通滤波与重采样，确保传递给大模型的语音特征不发生畸变。

### **信号量化与帧封包**

经过低通滤波与重采样后的数据依然是浮点格式，其振幅范围标准化在 \-1.0 到 1.0 之间。最后一步是将其量化为 16 位有符号整数。在转换前，为了防止极端峰值引起的数值溢出，必须对浮点信号进行硬限幅（Clipping）。随后，将浮点数乘以 32767.0（16位整数的最大正值），并将结果截断并转换为整数类型14。  
完成数据格式转换后，数据不可随意切分发送。为了维持服务端语音活动检测（VAD）算法的状态机稳定，音频数据需要按照固定的时间窗口进行封包。业内标准的实时语音流通常采用 40 毫秒至 100 毫秒的帧长度。在 16000 Hz 的采样率与 16 位（2 字节）深度的条件下，40 毫秒的音频精确对应 640 个采样点，即 1280 字节的二进制数据19。工作线程需将处理后的数据累积至 1280 字节，随后进行 Base64 编码，为网络传输做好准备16。

## **阶跃星辰端到端大模型的集成与网络拓扑**

系统在自然语言处理与语音识别层的核心驱动力来自于阶跃星辰发布的 StepAudio 2.5 Realtime 模型。与微软 Azure 等基于管线的传统语音服务相比，端到端模型展现出了压倒性的优势。在行业基准测试中，StepAudio 2.5 Realtime 展现了极低的延迟，其中位数延迟仅为 9.5 秒，远低于 Azure 的 73.7 秒，并在领域准确率上达到了 90.2%，优于 Azure 的 83.4%20。更重要的是，该模型在副语言（Paralinguistic）理解能力上取得了 82.18 分的优异成绩，能够精准感知说话者的语速、音调、疲劳度、挫败感乃至笑声与叹息20。通过面向角色扮演定制的强化学习人类反馈（RLHF）算法与百万级的特征矩阵扩增，模型在长时间交互中能够保持极高的角色一致性20。

### **WebSocket 握手与全双工信令协议**

为了充分发挥该模型的低延迟特性，系统底层摒弃了传统的 HTTP 短链接请求，转而采用 WebSocket 协议构建全双工（Full-duplex）持久连接。在 Rust 的 Tokio 异步运行时下，借助 tokio-tungstenite 库，系统能够建立到目标地址 wss://api.stepfun.com/v1/realtime/asr/stream 的连接16。  
建立连接时的身份鉴权依赖于 HTTP 协议升级请求阶段的 Header 注入。客户端必须在请求头中携带 Authorization: Bearer $STEPFUN\_API\_KEY，服务端会对该密钥进行严格的校验，只有合法的请求才能成功建立 WebSocket 会话16。  
连接建立的第一时间，客户端必须主动发送配置信令。这一名为 session.update 的指令向服务端确立了后续数据流的解码规则。  
下表展示了必须通过 WebSocket 发送的音频配置元数据结构：

| JSON 字段路径 | 数据类型 | 必需性 | 设定值与架构含义 |
| :---- | :---- | :---- | :---- |
| type | String | 必需 | 固定值为 "session.update"，触发服务端会话状态机重置。 |
| session.audio.input.format.type | String | 必需 | 设定为 "pcm"，告知模型舍弃容器头解析，直接处理裸流16。 |
| session.audio.input.format.codec | String | 必需 | 设定为 "pcm\_s16le"，明确字节序为小端，字长为 16 位16。 |
| session.audio.input.format.rate | Integer | 必需 | 设定为 16000，与客户端 DSP 降采样后的频率保持严格一致16。 |
| session.audio.input.format.bits | Integer | 必需 | 设定为 16，匹配量化深度16。 |
| session.audio.input.format.channel | Integer | 必需 | 设定为 1，匹配混音后的单声道属性16。 |

### **增量数据流传输与服务端 VAD 状态机**

在配置信令确认后，Rust 后端会进入一个高频的发送循环。每当 DSP 线程准备好一帧（如 40 毫秒）的音频数据并完成 Base64 编码后，Tokio 任务便会构造一个 input\_audio\_buffer.append 类型的 JSON 消息对象，并通过 WebSocket 通道将其推送到远端服务器16。  
由于 Stepfun 服务端内置了高精度的语音活动检测（VAD）算法，桌面客户端无需在本地耗费 CPU 资源去判断音频中是否存在人声。随着数据流的持续注入，服务端会异步地向客户端推送识别事件。这些事件主要分为两大类： 第一类是中间非稳态结果。随着模型逐渐接收到一个长句的音频，它会不断进行前向推理并修正之前的猜测，此时下发的文本是动态变化的，用于在前端渲染即时滚动的字幕草稿。 第二类是稳态结果。当服务端 VAD 检测到一段明显的静音（通常超过预设阈值），它会判定当前句子已经结束。此时服务端会下发带有精确起止时间戳与最终确定的文本结果，标志着当前段落识别周期的终结16。

## **Tauri v2 进程间通信与沉浸式前端渲染体系**

在全局字幕工具的设计中，数据流在到达 Rust 后端并完成解析后，必须以尽可能低的延迟穿越应用层边界，到达前端渲染引擎。Tauri v2 提供了一套基于操作系统原生消息队列的高性能进程间通信（IPC）机制，彻底解耦了后端的高强度并发运算与前端的图形界面渲染。

### **基于 Emitter 模式的全局事件总线**

在 Rust 代码中，每当 WebSocket 接收到阶跃星辰服务端下发的最新字幕片段时，系统会利用 Tauri 的 Emitter 接口，将数据结构序列化后广播给前端 Webview 实例24。  
开发者可以通过 AppHandle 实例的 emit 方法触发一个全局事件（例如命名为 subtitle-update）。无论是表示中间猜测的非稳态数据，还是代表段落结束的稳态数据，都被封装在统一定义的 Payload 中，附带标识符指示其状态。不仅如此，Tauri v2 还提供了细粒度的事件路由控制，例如利用 emit\_to 或 emit\_filter 方法，将敏感的调试日志仅发送给特定的监控窗口，而将字幕数据专职发送给用于显示字幕的透明窗口24。这种精准的事件分发极大地优化了多窗口架构下的性能。

### **响应式前端状态管理与监听器生命周期**

在前端（无论使用 Vue 3 的 Composition API 还是 React 的 Hooks），利用 @tauri-apps/api/event 模块提供的 listen 函数，可以建立对 Rust 广播事件的持续监听24。  
前端的状态管理逻辑通常维护两个变量：一个用于存储当前正在动态变化的句子（草稿），另一个用于存储已经由 VAD 判定结束的历史句子列表。当监听到 subtitle-update 事件时，前端根据 Payload 中的状态标志更新相应的变量，从而触发底层虚拟 DOM 的比对与界面的重绘。  
在前端架构设计中，必须极度重视事件监听器的生命周期管理。listen 函数是一个异步操作，它返回一个 Promise，该 Promise 最终解析为一个反注册函数（Unlisten function）24。如果组件在卸载时未能正确地同步等待该 Promise 解析并调用反注册函数，Tauri 的事件总线中将遗留孤立的回调引用。随着时间的推移，这将导致严重的内存泄漏，并使得同一个事件触发多次重复的渲染逻辑，最终引发前端卡顿崩溃。

### **操作系统级悬浮透明窗口工程实践**

为了达到类似 Wispr Flow 的无边框沉浸式字幕效果，必须在 Tauri 的配置文件（tauri.conf.json）与前端运行时进行深度的操作系统级窗口控制。  
首先，目标 Webview 被配置为 transparent: true 与 decorations: false。这两项设置移除了操作系统默认的窗口边框、标题栏以及 Webview 默认的白色不透明背景，使得 HTML 中的非绘制区域呈现为完全透明，仅保留字幕文本与特定的半透明底板。 其次，通过设置 alwaysOnTop: true，操作系统窗口管理器会强制将该字幕层置于所有其他应用程序之上，确保用户在观看视频或进行全屏展示时，字幕依然清晰可见。 更关键的是，为了防止悬浮的字幕层干扰用户对底层应用程序（如浏览器或游戏）的鼠标点击操作，系统需要在运行时动态调用 Tauri 的窗口 API，开启鼠标事件穿透（Ignore cursor events）。这种特权操作使得操作系统的命中测试（Hit-testing）算法忽略字幕窗口，直接将鼠标点击、滚动等事件传递给下方的原生应用层。

## **基于 SQLite 的本地数据持久化与高级分析引擎**

实时字幕的价值不仅在于当下的视觉辅助，更在于将流逝的语音信息转化为可长久检索、结构化分析的语料库。为了保障用户数据的绝对隐私控制权并支持复杂的关联查询，系统将所有的稳态语音识别结果与大模型推断出的副语言特征持久化至本地数据库中。

### **SQLite 在桌面应用架构中的不可替代性**

在构建 Tauri 桌面应用时，开发者通常会在轻量级的 JSON 文件存储与关系型数据库之间进行权衡。对于需要记录海量历史流式数据的字幕工具而言，JSON 文件的序列化反序列化开销、缺乏索引导致的极慢查询速度，以及在多线程高频写入下的文件损坏风险（竞争条件），使其完全无法胜任此类任务。  
SQLite 凭借其零运维、单文件部署、原生 ACID 事务支持等特性，成为了桌面应用持久化的行业标准27。在 Rust 后端中，系统集成了 rusqlite 或更现代的完全异步库 sqlx。sqlx 不仅不会阻塞 Tokio 的异步运行时环境，还能够利用编译期宏验证 SQL 语句的安全性和正确性28。

### **数据库初始化、隔离路径与状态共享**

Tauri 框架提供了严格且安全的路径解析 API。通过 app\_handle.path().app\_data\_dir()，系统能够自动根据当前运行的操作系统（如 macOS 的 \~/Library/Application Support/ 或 Windows 的 %APPDATA%）定位到专属的应用数据沙盒目录27。  
在应用首次启动时，Rust 后端会检查沙盒目录下是否存在 SQLite 数据库文件。如果不存在，则触发初始化流程。借助 sqlx 强大的迁移（Migration）机制，通过 sqlx::migrate\!("./migrations").run(\&pool).await 指令，系统可以按顺序安全地执行版本化的 DDL 语句，建立所需的数据表与索引28。  
建立数据库连接池（Pool）后，该池对象将被包裹在 Tauri 的托管状态（Managed State）机制中。如果使用的是同步的 rusqlite，连接通常需要用 Mutex\<Connection\> 进行跨线程互斥保护；而如果是 sqlx 的连接池，由于其内部已经实现了高效的并发控制与连接复用，只需将其直接注入 Tauri 的全局状态即可。这使得任何其他的异步 Command（例如供前端调用的查询接口）都能随时随地地安全获取数据库句柄27。

### **数据表拓扑与多模态元数据留存**

数据表的设计不仅是对单纯文本的记录，更是对阶跃星辰模型丰富输出的深度留存。一张标准的记录表包含以下维度的数据结构：

| 数据库列名 | 存储数据类型 | 索引/约束 | 业务逻辑与分析价值 |
| :---- | :---- | :---- | :---- |
| id | INTEGER | PRIMARY KEY | 自增主键，保障单调递增性。 |
| session\_guid | TEXT | INDEX | 唯一的会话标识符。用于在庞大的数据库中快速过滤并重构某一次特定的会议或播客的完整对话时间轴。 |
| transcription\_text | TEXT | \- | 稳态的语音识别最终文本内容。 |
| start\_timestamp | BIGINT | \- | 基于 VAD 机制产生的句子起始绝对时间戳，用于与外部媒体文件进行对齐。 |
| end\_timestamp | BIGINT | \- | 句子结束的绝对时间戳。 |
| paralinguistic\_metadata | TEXT | \- | 存储为 JSON 字符串。依赖 StepAudio 强大的副语言识别能力，记录捕捉到的情绪（如沮丧、喜悦）、语速变化或非语言事件（笑声）20。这为后续大语言模型对长文本执行摘要总结或情感分析提供了极其关键的深层特征输入。 |

通过利用 \#\[tauri::command\] 宏，开发者可以轻松地将诸如 fetch\_history(session\_id: String) 或 search\_keywords(keyword: String) 这样的 Rust 数据库查询方法暴露给前端的 JavaScript 运行时。借由 sqlx::query\_as\! 宏的自动反序列化能力，前端可以直接接收到类型安全的强结构数据，在零解析成本的前提下实现历史字幕界面的极速渲染与无缝滚动28。

## **系统监控、容错机制与日志审计**

作为一款需要在后台持续拦截系统音频并与外部网络保持长时间 WebSocket 连接的常驻守护进程，任何细微的异常都可能导致进程崩溃或服务静默失效。因此，完善的容错与全链路监控机制是工程实施中不可或缺的闭环。  
在网络层面，WebSocket 长连接对网络波动极为敏感。Rust 后端的 Tokio 任务中部署了指数退避（Exponential Backoff）重连算法。当检测到连接由于超时或网络故障而断开时，系统将暂停向解码管道输入音频缓冲，并在等待逐渐递增的时间间隔后尝试重建认证连接。在断开期间，少量的音频数据可缓存在内存中以防丢失，并在重连成功后触发批量补发逻辑。  
在日志审计方面，系统深度集成了官方支持的 tauri-plugin-log 日志收集分析插件32。通过向 Tauri 的 Builder 注册该插件并进行定制化配置，系统可以将原本分散在各处的日志统一管理。例如，可以配置目标为 TargetKind::LogDir，使得所有的日志自动持久化写入操作系统规范的日志专有目录（如 Linux 上的 $HOME/.local/share/{bundleIdentifier}/logs）32。  
为了防止长时间运行导致日志文件无限制膨胀挤占用户磁盘空间，系统通过 .max\_file\_size(50\_000) 方法设定日志的触发切分大小，启动了自动的日志轮转（Log Rotation）机制32。更为精妙的是，利用该插件提供的拦截器（Interceptor），通过调用 forwardConsole 函数，可以将前端运行时的 console.log、console.error 拦截，并通过 Tauri 的 IPC 通道同步写入到 Rust 掌管的同一物理日志文件中32。同时，通过配置 timezone\_strategy(tauri\_plugin\_log::TimezoneStrategy::UseLocal)，确保前后端日志在时间轴上拥有统一的本地时间戳基准32。这一机制彻底打通了从前端渲染异常到后端 SQLite 读写错误，再到 cpal 底层音频掉帧报错的全链路追踪能力，极大地缩短了开发者在系统崩溃后的故障根因分析（RCA）耗时。

## **结语**

通过融合 Tauri v2 的现代化桌面架构、Rust 的底层系统级音频捕获能力以及阶跃星辰端到端语音大模型，构建全局捕获与实时字幕渲染系统不仅在技术上完全可行，而且在延迟与准确度等核心指标上展现出了相较于传统技术栈的代际优势。系统的成功实施依赖于跨越底层声学设备（WASAPI/ScreenCaptureKit/ALSA）的严密数字信号处理，基于 WebSocket 的高并发全双工通信，以及基于 SQLite 的无缝持久化引擎。该架构的设计不仅满足了实时字幕的即时展示需求，更利用 StepAudio 卓越的副语言理解能力构建了结构化的高质量语料资产，为探索桌面级人工智能助手在多模态理解与深度会议纪要分析等衍生应用场景指明了全新的演进方向。

#### **引用的著作**

1. wasapi \- crates.io: Rust Package Registry, [https://crates.io/crates/wasapi](https://crates.io/crates/wasapi)  
2. Support WASAPI Loopback · Issue \#251 · RustAudio/cpal \- GitHub, [https://github.com/tomaka/cpal/issues/251](https://github.com/tomaka/cpal/issues/251)  
3. s0d3s/PyAudioWPatch: PyAudio | PortAudio fork with WASAPI loopback support Record audio from speakers on Windows · GitHub, [https://github.com/s0d3s/PyAudioWPatch](https://github.com/s0d3s/PyAudioWPatch)  
4. PyAudioWPatch \- PyPI, [https://pypi.org/project/PyAudioWPatch/](https://pypi.org/project/PyAudioWPatch/)  
5. PyAudioWPatch/examples/pawp\_record\_wasapi\_loopback.py at master \- GitHub, [https://github.com/s0d3s/PyAudioWPatch/blob/master/examples/pawp\_record\_wasapi\_loopback.py](https://github.com/s0d3s/PyAudioWPatch/blob/master/examples/pawp_record_wasapi_loopback.py)  
6. Loopback: Combining microphone and application audio in screen recordings, [https://rogueamoeba.com/support/knowledgebase/?showArticle=Loopback-ScreenRecording](https://rogueamoeba.com/support/knowledgebase/?showArticle=Loopback-ScreenRecording)  
7. Details on Loopback's audio handling on MacOS 14 and higher \- Rogue Amoeba Support, [https://rogueamoeba.com/support/knowledgebase/?showArticle=Misc-ARK-Plugin-Audio-Capture-Details\&product=Loopback](https://rogueamoeba.com/support/knowledgebase/?showArticle=Misc-ARK-Plugin-Audio-Capture-Details&product=Loopback)  
8. How to Record Mac System Audio Using Python and BlackHole | by Mehdi Samadi, [https://medium.com/@mehsamadi/how-to-record-mac-system-audio-using-python-and-blackhole-a45d06eaad0f](https://medium.com/@mehsamadi/how-to-record-mac-system-audio-using-python-and-blackhole-a45d06eaad0f)  
9. Suppport ScreenCaptureKit loopback · Issue \#876 · RustAudio/cpal \- GitHub, [https://github.com/RustAudio/cpal/issues/876](https://github.com/RustAudio/cpal/issues/876)  
10. Support ScreenCapture loopback by Kree0 · Pull Request \#894 · RustAudio/cpal \- GitHub, [https://github.com/RustAudio/cpal/pull/894](https://github.com/RustAudio/cpal/pull/894)  
11. How do I record system audio in Python? (Linux) \- Stack Overflow, [https://stackoverflow.com/questions/53902065/how-do-i-record-system-audio-in-python-linux](https://stackoverflow.com/questions/53902065/how-do-i-record-system-audio-in-python-linux)  
12. RustAudio/cpal: Low-level cross-platform audio I/O library in Rust \- GitHub, [https://github.com/RustAudio/cpal](https://github.com/RustAudio/cpal)  
13. cpal \- Rust \- Docs.rs, [https://docs.rs/cpal/latest/cpal/](https://docs.rs/cpal/latest/cpal/)  
14. Convert audio data of PCM16/float32 to byte, and vice versa. \- Gist \- GitHub, [https://gist.github.com/HudsonHuang/fbdf8e9af7993fe2a91620d3fb86a182](https://gist.github.com/HudsonHuang/fbdf8e9af7993fe2a91620d3fb86a182)  
15. cpal/examples/feedback.rs at master · RustAudio/cpal \- GitHub, [https://github.com/RustAudio/cpal/blob/master/examples/feedback.rs](https://github.com/RustAudio/cpal/blob/master/examples/feedback.rs)  
16. 流式语音识别（双向流式） \- StepFun 开放平台文档中心, [https://platform.stepfun.com/docs/zh/api-reference/audio/asr-stream](https://platform.stepfun.com/docs/zh/api-reference/audio/asr-stream)  
17. ue4 Convert audio from 48 stereo to 16 mono \- Stack Overflow, [https://stackoverflow.com/questions/69085916/ue4-convert-audio-from-48-stereo-to-16-mono](https://stackoverflow.com/questions/69085916/ue4-convert-audio-from-48-stereo-to-16-mono)  
18. Downsampling wav audio file \- python \- Stack Overflow, [https://stackoverflow.com/questions/30619740/downsampling-wav-audio-file](https://stackoverflow.com/questions/30619740/downsampling-wav-audio-file)  
19. 实时语音识别（Websocket）, [https://www.tencentcloud.com/zh/document/product/1118/53937](https://www.tencentcloud.com/zh/document/product/1118/53937)  
20. StepFun Releases StepAudio 2.5 Realtime: An End-to-End Voice Model with Roleplay-Specific RLHF and Paralinguistic Comprehension \- MarkTechPost, [https://www.marktechpost.com/2026/05/24/stepfun-releases-stepaudio-2-5-realtime-an-end-to-end-voice-model-with-roleplay-specific-rlhf-and-paralinguistic-comprehension/](https://www.marktechpost.com/2026/05/24/stepfun-releases-stepaudio-2-5-realtime-an-end-to-end-voice-model-with-roleplay-specific-rlhf-and-paralinguistic-comprehension/)  
21. StepAudio 2.5 Realtime：角色一致性与情感感知领先的实时语音模型 \- YouTube, [https://www.youtube.com/watch?v=fQoqRl5M0EM](https://www.youtube.com/watch?v=fQoqRl5M0EM)  
22. stepfun-ai/Step-Realtime-Console \- GitHub, [https://github.com/stepfun-ai/Step-Realtime-Console/blob/main/README-en.md](https://github.com/stepfun-ai/Step-Realtime-Console/blob/main/README-en.md)  
23. 双向实时语音- StepFun 开放平台文档中心, [https://platform.stepfun.com/docs/zh/api-reference/realtime/chat](https://platform.stepfun.com/docs/zh/api-reference/realtime/chat)  
24. Calling the Frontend from Rust | Tauri, [https://v2.tauri.app/develop/calling-frontend/](https://v2.tauri.app/develop/calling-frontend/)  
25. Handling events in Tauri \- Tauri Tutorials, [https://tauritutorials.com/blog/tauri-events-basics](https://tauritutorials.com/blog/tauri-events-basics)  
26. Calling the Frontend from Rust | Tauri, [https://v2.tauri.app/develop/\_sections/frontend-listen/](https://v2.tauri.app/develop/_sections/frontend-listen/)  
27. SQLite in a Tauri v2 App — Simple, Reliable, Zero Regrets \- DEV Community, [https://dev.to/hiyoyok/sqlite-in-a-tauri-v2-app-simple-reliable-zero-regrets-391h](https://dev.to/hiyoyok/sqlite-in-a-tauri-v2-app-simple-reliable-zero-regrets-391h)  
28. Building a todo app in Tauri with SQLite and sqlx, [https://tauritutorials.com/blog/building-a-todo-app-in-tauri-with-sqlite-and-sqlx](https://tauritutorials.com/blog/building-a-todo-app-in-tauri-with-sqlite-and-sqlx)  
29. Embedding a SQLite database in a Tauri Application : r/rust \- Reddit, [https://www.reddit.com/r/rust/comments/1hrsovh/embedding\_a\_sqlite\_database\_in\_a\_tauri\_application/](https://www.reddit.com/r/rust/comments/1hrsovh/embedding_a_sqlite_database_in_a_tauri_application/)  
30. \[sql\] Where is database saved? · Issue \#1653 · tauri-apps/plugins-workspace \- GitHub, [https://github.com/tauri-apps/plugins-workspace/issues/1653](https://github.com/tauri-apps/plugins-workspace/issues/1653)  
31. Calling Rust from the Frontend \- Tauri, [https://v2.tauri.app/develop/calling-rust/](https://v2.tauri.app/develop/calling-rust/)  
32. Logging \- Tauri, [https://v2.tauri.app/plugin/logging/](https://v2.tauri.app/plugin/logging/)