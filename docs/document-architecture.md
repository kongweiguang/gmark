<!-- @author kongweiguang -->

# 文档引擎架构

## 不变量

- 源码文本和 revision 是唯一文档真值；表格、JSON 图和 Markdown 块都是可重建投影。
- 格式决定视图能力，资源画像决定 Resident 或 Paged 后端，两者不得互相代替。
- 投影只读取任务启动时的不可变快照；编辑必须返回带 base revision 的 Source transaction。
- 任一打开阈值越界都选择 Paged Source，且不启动需要全文扫描的结构投影。
- 保存失败不得改变 dirty、revision 或恢复基线。

## 分层

1. `gmark-document-core`：格式画像、加载预设、打开计划、字节事务、快照、View Registry/Provider、恢复后端契约和领域错误。Provider 统一返回 `ProjectionError`，恢复统一接收 Source transaction、byte selection 与稳定 View ID。
2. `gmark-document`：普通文件的 Resident Rope、事务、撤销/重做与格式保真。
3. `gmark-paged-document`：大文件 Paged PieceDocument、行索引、有界搜索、流式保存和恢复。
4. `gmark-document-runtime`：显式 `DocumentStore::{Resident, Paged}` 与 `DocumentSession`，统一 revision、transaction、selection、dirty、文件身份和允许视图。
5. 格式 Provider：Markdown、Delimited、JSON/JSONL 只消费通用快照，不持有 GPUI Entity 或后端实体；Resident CSV/JSONL 直接使用 `Arc<[u8]>`，不创建结构影子文件。
6. 应用层：Editor 与 Tab 快照直接持有 `Option<Entity<DocumentHost>>`，不存在中间 Surface/backend enum。Resident Markdown 由 Editor controller 消费同一 `DocumentSession`，其它普通格式与 Paged Source 由 `DocumentHost` 承载；Resident 的 Source 选择写回 `DocumentSession.view_state`，UI 不另存正文、revision、dirty、active view 或可恢复选择真值。初次加载及保存暂时移出 session 时，Host 只持有互斥的 pending view/dirty 槽，安装 session 时原子转交，Ready 状态 pending 槽必须为空。
   Markdown 打开结果同时携带 Probe 的真实 encoding 与 file identity，禁止用空身份或固定 UTF-8 构造已落盘 Session；只有 Untitled 会话允许空路径身份。
7. 运行时协调：`DocumentCoordinator` 统一持有索引、Source、搜索、外部监控和恢复任务的 generation/cancellation；其中 `SaveCoordinator` 独立拥有保存代次、取消令牌和任务。打开决策不进入 GPUI controller，Controller 只负责把输入转换为 session command。

## 打开与降级

Balanced 默认线为 16 MiB、100,000 行和 500,000 结构单元；Low Memory 为 8 MiB/50,000/250,000；High Performance 为 64 MiB/250,000/1,000,000。用户可在 `[documents.loading]` 覆盖三项限制，非法字段逐项回退预设并保留校验提示。设置只影响下次打开或显式 Reload；普通保存不会改变当前后端。

有界 Probe 记录规范路径、长度、mtime 与平台文件 ID。完整读取前后都重新校验 identity；打开期间发生增长或同长度替换时拒绝过期结果，使用原 ProbeOptions 最多重新规划三次。安全 Source 标记只属于本次 Probe，不写回全局偏好。

Resident 会话冻结打开时的有效阈值，并维护当前字节数、精确行数和结构单元数。首次越界只记录稳定提示，不热迁移；保存仍保留当前后端并完成回读，下次打开再按磁盘文件重新规划。

Paged 文档只提供 Source，但保留编辑、撤销/重做、搜索、定位、恢复与原子保存。普通文件按格式选择 Markdown Live、Delimited Table、JSON Graph、JSONL Structure 或 Source。

应用打开结果使用 `Resident`、`ResidentFormat` 与 `Paged` 三个明确分支；禁止再用 `SourceBacked` 同时表示普通格式宿主和超大文件后端。Paged 会话在首次索引、外部追加、撤销恢复、重新加载和保存后重建阶段都不得启动结构投影。

## 保存、恢复与会话兼容

Resident Editor 使用不可变 Rope 快照，DocumentHost 使用统一 `DocumentSession` 快照；两条路径都执行 identity/冲突检查、同目录临时文件、原子替换、持久化同步和保存后回读。回读与保存快照任一字节不一致时保持 dirty、revision 与恢复日志，不安装新基线。

Resident 与 Paged 日志保留各自兼容编码，但都实现 `RecoveryBackend`。Paged Controller 不再调用后端专用 `record_replace/undo/redo`，只提交统一 `RecoveryRecord`。

Workspace session 不持久化后端。旧 `Live/Source/Preview/Structure/Split`（不区分大小写）映射到稳定视图；重新规划为 Paged 时强制 Source，但恢复源码选择、anchor affinity 与纵向位置。

## 滚动契约

普通滚轮只驱动纵向；Shift/Ctrl + 纵向滚轮映射到横向；触控板原生横向 delta 保持横向。GPUI 横向 overflow 必须开启 `restrict_scroll_to_axis`，避免将无修饰键的纵向滚轮自动转为横向。

## 本地诊断与隐私

文档诊断默认关闭；仅在进程显式设置 `GMARK_PERF_TRACE=1` 时，以一行一个 JSON 的形式写入 stderr。记录范围限定为格式、Resident/Paged 选择、降级原因、Probe 与 GPUI 首帧耗时、Source/布局缓存峰值、投影和视口取消计数、保存及恢复结果。诊断接口只接收封闭领域枚举、数值和固定状态标签，不接收正文、搜索词、文件路径或任意错误字符串。
