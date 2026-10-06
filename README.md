# ting-plugin-sdk

用于开发 Ting Reader 的 WASM 和 Native 插件，提供业务入口、Host 调用、资源读写、HTTP 请求和运行时导出。

## 添加依赖

推荐使用 Rust 1.93 或更高版本。在插件的 `Cargo.toml` 中添加：

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
ting-plugin-sdk = { git = "https://github.com/dqsq2e2/ting-plugin-sdk.git", tag = "v2.0.3" }
serde_json = "1"
```

## 实现业务

下面的 `src/lib.rs` 实现一个工具操作：

```rust
use serde_json::{Value, json};
use ting_plugin_sdk::{Host, Plugin, Result, SdkError};
use ting_plugin_sdk::contract::protocol::PluginErrorCode;

#[derive(Default)]
struct MyPlugin;

impl Plugin for MyPlugin {
    const ID: &'static str = "my-plugin";
    const OPERATIONS: &'static [&'static str] = &["invokeTool"];

    fn invoke(&mut self, operation: &str, input: Value, _host: &dyn Host) -> Result<Value> {
        match operation {
            "invokeTool" => Ok(json!({ "message": input["params"]["message"] })),
            _ => Err(SdkError::new(
                PluginErrorCode::UnsupportedOperation,
                "Unsupported operation",
            )),
        }
    }
}

ting_plugin_sdk::export_plugin!(MyPlugin);
```

在 `plugin.yml` 中填写相同的插件 ID，声明 `tool_provider` 能力、`invokeTool` 操作及工具输入输出 Schema。元数据搜索插件可使用 [ting-scraper-sdk](https://github.com/dqsq2e2/ting-scraper-sdk) 的结果转换函数。

## 调用 Host

`host.invoke(method, params)` 访问宿主业务接口，所需权限写入插件清单。`host_call` 可以把响应转换为指定 Rust 类型。

- `http_request_response` 返回源站状态码和响应正文；`http_request` 返回正文。
- `Host::read_at` 和 `read_exact_range` 读取宿主资源；`Host::write_at` 写入暂存输出。
- `Host::chunk_create` 和 `chunk_copy` 处理二进制块。

资源和块使用完后调用对应的关闭、释放接口。Host 身份、权限和资源作用域由服务端校验。

## 构建和安装

WASM：

```sh
rustup target add wasm32-wasip1
cargo check --target wasm32-wasip1
cargo build --release --target wasm32-wasip1
```

Native：

```sh
cargo check
cargo build --release
```

将产物放到清单 `entry_point` 指定的位置，使用 `trpack validate`、`trpack build --sign-key`、`trpack verify`，再在测试服务端安装 `.tr` 包并调用实际功能。

完整流程见 [插件开发指南](https://github.com/dqsq2e2/ting-reader/blob/chore/rust-2024-edition/docs/plugins/plugin-dev.md)，接口与权限见同目录的能力、Host 和运行时文档。

## JavaScript

`javascript/sdk.mjs` 提供 `success`、`failure`、`publishSearch` 和资源辅助函数，`javascript/sdk.d.ts` 提供类型声明。插件业务入口按相对路径导入：

```js
import { success, publishSearch } from './sdk.mjs';
```

## 验证本仓库

```sh
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo check --locked --target wasm32-wasip1
```
