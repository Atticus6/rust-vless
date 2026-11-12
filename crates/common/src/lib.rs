//! 主节点 API 与子节点共用的 VLESS 数据面类型。

pub mod protocol;
pub mod registry;
pub mod util;

pub use protocol::{
    parse_request, Command, ParseError, ParsedRequest, MAX_WS_MESSAGE, VLESS_VERSION,
};
pub use registry::{UserRegistry, UserStats};

/// 二进制的发布版本，唯一来源是仓库根目录的 `version.txt`。
///
/// 通过 `include_str!` 编译进二进制，因此发布包里的版本号与实际构建一致；
/// 文件以换行结尾，这里在运行时去掉空白。
pub fn version() -> &'static str {
    include_str!("../../../version.txt").trim()
}
