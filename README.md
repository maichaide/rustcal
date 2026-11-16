# RustCal v0.3

基于 **Rust + egui + SQLite** 开发的跨平台现代 GUI 日历与任务管理软件，完整复刻 `calcurse` 核心能力并扩展现代日历生态。

## v0.3 核心修复与升级
1. **异步后台导入 + 实时进度条 + SQLite 事务批量写入（彻底解决大文件假死）**：
   - 导入包含数千条日程的 `.ics` / `.json` 文件时，置于后台独立线程执行解码、解析与 SQLite 单事务（`Transaction` + 预编译 `Statement`）批量入库，数千条日程入库耗时从数秒降至几十毫秒。
   - UI 界面弹出实时进度条模态窗（展示当前阶段、百分比及已处理/总条数），并通过 `ctx.request_repaint()` 保持界面流畅响应，绝不假死。
2. **彻底修复中文乱码问题（双层防护：解码 + CJK 字体渲染）**：
   - **解码层修复**：修复此前无 BOM `UTF-16LE` 启发式误判逻辑（英文 ICS 关键词含 ASCII 偶数字节特征可能干扰判定），改用精确验证解码结果中是否包含 `BEGIN:VCALENDAR` / `VEVENT` 结构标记来校验 UTF-16LE/BE、UTF-8、GB18030/GBK、Big5、Shift_JIS 等编码；同时正确处理 RFC 5545 多字节 UTF-8/GBK 折行（先按原始字节展开折行，再整体解码，避免多字节汉字在 75 字节处被折行截断导致乱码！）。
   - **渲染层修复**：扩展跨平台 CJK 字体探测路径（覆盖 Windows 10/11 微软雅黑/黑体/宋体/楷体、macOS 苹方/华文黑体、Linux NotoSansCJK/WenQuanYi），并同时注册到 `Proportional` 与 `Monospace` 字体族首位，启动时校验字体加载状态。

## 编译运行
```bash
cargo build --release
cargo run --release
```
