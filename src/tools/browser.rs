//! Browser automation tool with pluggable backends.
//!
//! By default this uses Vercel's `agent-browser` CLI for automation.
//! Optionally, a Rust-native backend can be enabled at build time via
//! `--features browser-native` and selected through config.

use super::traits::{Tool, ToolResult};
use crate::security::SecurityPolicy;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tracing::debug;

/// Browser automation tool using agent-browser CLI
pub struct BrowserTool {
    security: Arc<SecurityPolicy>,
    allowed_domains: Vec<String>,
    session_name: Option<String>,
    backend: String,
    native_headless: bool,
    native_chrome_path: Option<String>,
    #[cfg(feature = "browser-native")]
    native_state: std::sync::Mutex<native_backend::NativeBrowserState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserBackendKind {
    AgentBrowser,
    RustNative,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedBackend {
    AgentBrowser,
    RustNative,
}

impl BrowserBackendKind {
    fn parse(raw: &str) -> anyhow::Result<Self> {
        let key = raw.trim().to_ascii_lowercase().replace('-', "_");
        match key.as_str() {
            "agent_browser" | "agentbrowser" => Ok(Self::AgentBrowser),
            "rust_native" | "native" => Ok(Self::RustNative),
            "auto" => Ok(Self::Auto),
            _ => anyhow::bail!(
                "Unsupported browser backend '{raw}'. Use 'agent_browser', 'rust_native', or 'auto'"
            ),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::AgentBrowser => "agent_browser",
            Self::RustNative => "rust_native",
            Self::Auto => "auto",
        }
    }
}

/// Response from agent-browser --json commands
#[derive(Debug, Deserialize)]
struct AgentBrowserResponse {
    success: bool,
    data: Option<Value>,
    error: Option<String>,
}

/// Supported browser actions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserAction {
    /// Navigate to a URL
    Open { url: String },
    /// Get accessibility snapshot with refs
    Snapshot {
        #[serde(default)]
        interactive_only: bool,
        #[serde(default)]
        compact: bool,
        #[serde(default)]
        depth: Option<u32>,
    },
    /// Click an element by ref or selector
    Click { selector: String },
    /// Fill a form field
    Fill { selector: String, value: String },
    /// Type text into focused element
    Type { selector: String, text: String },
    /// Get text content of element
    GetText { selector: String },
    /// Get page title
    GetTitle,
    /// Get current URL
    GetUrl,
    /// Take screenshot
    Screenshot {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        full_page: bool,
    },
    /// Wait for element or time
    Wait {
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        ms: Option<u64>,
        #[serde(default)]
        text: Option<String>,
    },
    /// Press a key
    Press { key: String },
    /// Hover over element
    Hover { selector: String },
    /// Scroll page
    Scroll {
        direction: String,
        #[serde(default)]
        pixels: Option<u32>,
    },
    /// Check if element is visible
    IsVisible { selector: String },
    /// Close browser
    Close,
    /// Find element by semantic locator
    Find {
        by: String, // role, text, label, placeholder, testid
        value: String,
        action: String, // click, fill, text, hover
        #[serde(default)]
        fill_value: Option<String>,
    },
}

impl BrowserTool {
    pub fn new(
        security: Arc<SecurityPolicy>,
        allowed_domains: Vec<String>,
        session_name: Option<String>,
    ) -> Self {
        Self::new_with_backend(
            security,
            allowed_domains,
            session_name,
            "agent_browser".into(),
            true,
            None,
        )
    }

    pub fn new_with_backend(
        security: Arc<SecurityPolicy>,
        allowed_domains: Vec<String>,
        session_name: Option<String>,
        backend: String,
        native_headless: bool,
        native_chrome_path: Option<String>,
    ) -> Self {
        Self {
            security,
            allowed_domains: normalize_domains(allowed_domains),
            session_name,
            backend,
            native_headless,
            native_chrome_path,
            #[cfg(feature = "browser-native")]
            native_state: std::sync::Mutex::new(native_backend::NativeBrowserState::default()),
        }
    }

    /// Check if agent-browser CLI is available
    pub async fn is_agent_browser_available() -> bool {
        Command::new("agent-browser")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Backward-compatible alias.
    pub async fn is_available() -> bool {
        Self::is_agent_browser_available().await
    }

    fn configured_backend(&self) -> anyhow::Result<BrowserBackendKind> {
        BrowserBackendKind::parse(&self.backend)
    }

    fn rust_native_compiled() -> bool {
        cfg!(feature = "browser-native")
    }

    fn rust_native_available(&self) -> bool {
        #[cfg(feature = "browser-native")]
        {
            native_backend::NativeBrowserState::is_available(
                self.native_headless,
                self.native_chrome_path.as_deref(),
            )
        }
        #[cfg(not(feature = "browser-native"))]
        {
            false
        }
    }

    async fn resolve_backend(&self) -> anyhow::Result<ResolvedBackend> {
        let configured = self.configured_backend()?;

        match configured {
            BrowserBackendKind::AgentBrowser => {
                if Self::is_agent_browser_available().await {
                    Ok(ResolvedBackend::AgentBrowser)
                } else {
                    anyhow::bail!(
                        "browser.backend='{}' but agent-browser CLI is unavailable. Install with: npm install -g agent-browser",
                        configured.as_str()
                    )
                }
            }
            BrowserBackendKind::RustNative => {
                if !Self::rust_native_compiled() {
                    anyhow::bail!(
                        "browser.backend='rust_native' requires build feature 'browser-native'"
                    );
                }
                if !self.rust_native_available() {
                    anyhow::bail!(
                        "Rust-native browser backend is enabled but no Chrome/Chromium executable was found"
                    );
                }
                Ok(ResolvedBackend::RustNative)
            }
            BrowserBackendKind::Auto => {
                if Self::rust_native_compiled() && self.rust_native_available() {
                    return Ok(ResolvedBackend::RustNative);
                }
                if Self::is_agent_browser_available().await {
                    return Ok(ResolvedBackend::AgentBrowser);
                }

                if Self::rust_native_compiled() {
                    anyhow::bail!(
                        "browser.backend='auto' found no usable backend (agent-browser missing, rust-native unavailable)"
                    )
                }

                anyhow::bail!(
                    "browser.backend='auto' needs agent-browser CLI, or build with --features browser-native"
                )
            }
        }
    }

    /// Validate URL against allowlist
    fn validate_url(&self, url: &str) -> anyhow::Result<()> {
        let url = url.trim();

        if url.is_empty() {
            anyhow::bail!("URL cannot be empty");
        }

        // Allow file:// URLs for local testing
        if url.starts_with("file://") {
            return Ok(());
        }

        if !url.starts_with("https://") && !url.starts_with("http://") {
            anyhow::bail!("Only http:// and https:// URLs are allowed");
        }

        if self.allowed_domains.is_empty() {
            anyhow::bail!(
                "Browser tool enabled but no allowed_domains configured. \
                Add [browser].allowed_domains in config.toml"
            );
        }

        let host = extract_host(url)?;

        if is_private_host(&host) {
            anyhow::bail!("Blocked local/private host: {host}");
        }

        if !host_matches_allowlist(&host, &self.allowed_domains) {
            anyhow::bail!("Host '{host}' not in browser.allowed_domains");
        }

        Ok(())
    }

    /// Execute an agent-browser command
    async fn run_command(&self, args: &[&str]) -> anyhow::Result<AgentBrowserResponse> {
        let mut cmd = Command::new("agent-browser");

        // Add session if configured
        if let Some(ref session) = self.session_name {
            cmd.arg("--session").arg(session);
        }

        // Add --json for machine-readable output
        cmd.args(args).arg("--json");

        debug!("Running: agent-browser {} --json", args.join(" "));

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stderr.is_empty() {
            debug!("agent-browser stderr: {}", stderr);
        }

        // Parse JSON response
        if let Ok(resp) = serde_json::from_str::<AgentBrowserResponse>(&stdout) {
            return Ok(resp);
        }

        // Fallback for non-JSON output
        if output.status.success() {
            Ok(AgentBrowserResponse {
                success: true,
                data: Some(json!({ "output": stdout.trim() })),
                error: None,
            })
        } else {
            Ok(AgentBrowserResponse {
                success: false,
                data: None,
                error: Some(stderr.trim().to_string()),
            })
        }
    }

    /// Execute a browser action via agent-browser CLI
    #[allow(clippy::too_many_lines)]
    async fn execute_agent_browser_action(
        &self,
        action: BrowserAction,
    ) -> anyhow::Result<ToolResult> {
        match action {
            BrowserAction::Open { url } => {
                self.validate_url(&url)?;
                let resp = self.run_command(&["open", &url]).await?;
                self.to_result(resp)
            }

            BrowserAction::Snapshot {
                interactive_only,
                compact,
                depth,
            } => {
                let mut args = vec!["snapshot"];
                if interactive_only {
                    args.push("-i");
                }
                if compact {
                    args.push("-c");
                }
                let depth_str;
                if let Some(d) = depth {
                    args.push("-d");
                    depth_str = d.to_string();
                    args.push(&depth_str);
                }
                let resp = self.run_command(&args).await?;
                self.to_result(resp)
            }

            BrowserAction::Click { selector } => {
                let resp = self.run_command(&["click", &selector]).await?;
                self.to_result(resp)
            }

            BrowserAction::Fill { selector, value } => {
                let resp = self.run_command(&["fill", &selector, &value]).await?;
                self.to_result(resp)
            }

            BrowserAction::Type { selector, text } => {
                let resp = self.run_command(&["type", &selector, &text]).await?;
                self.to_result(resp)
            }

            BrowserAction::GetText { selector } => {
                let resp = self.run_command(&["get", "text", &selector]).await?;
                self.to_result(resp)
            }

            BrowserAction::GetTitle => {
                let resp = self.run_command(&["get", "title"]).await?;
                self.to_result(resp)
            }

            BrowserAction::GetUrl => {
                let resp = self.run_command(&["get", "url"]).await?;
                self.to_result(resp)
            }

            BrowserAction::Screenshot { path, full_page } => {
                let mut args = vec!["screenshot"];
                if let Some(ref p) = path {
                    args.push(p);
                }
                if full_page {
                    args.push("--full");
                }
                let resp = self.run_command(&args).await?;
                self.to_result(resp)
            }

            BrowserAction::Wait { selector, ms, text } => {
                let mut args = vec!["wait"];
                let ms_str;
                if let Some(sel) = selector.as_ref() {
                    args.push(sel);
                } else if let Some(millis) = ms {
                    ms_str = millis.to_string();
                    args.push(&ms_str);
                } else if let Some(ref t) = text {
                    args.push("--text");
                    args.push(t);
                }
                let resp = self.run_command(&args).await?;
                self.to_result(resp)
            }

            BrowserAction::Press { key } => {
                let resp = self.run_command(&["press", &key]).await?;
                self.to_result(resp)
            }

            BrowserAction::Hover { selector } => {
                let resp = self.run_command(&["hover", &selector]).await?;
                self.to_result(resp)
            }

            BrowserAction::Scroll { direction, pixels } => {
                let mut args = vec!["scroll", &direction];
                let px_str;
                if let Some(px) = pixels {
                    px_str = px.to_string();
                    args.push(&px_str);
                }
                let resp = self.run_command(&args).await?;
                self.to_result(resp)
            }

            BrowserAction::IsVisible { selector } => {
                let resp = self.run_command(&["is", "visible", &selector]).await?;
                self.to_result(resp)
            }

            BrowserAction::Close => {
                let resp = self.run_command(&["close"]).await?;
                self.to_result(resp)
            }

            BrowserAction::Find {
                by,
                value,
                action,
                fill_value,
            } => {
                let mut args = vec!["find", &by, &value, &action];
                if let Some(ref fv) = fill_value {
                    args.push(fv);
                }
                let resp = self.run_command(&args).await?;
                self.to_result(resp)
            }
        }
    }

    fn execute_rust_native_action(&self, action: BrowserAction) -> anyhow::Result<ToolResult> {
        #[cfg(feature = "browser-native")]
        {
            let mut state = self
                .native_state
                .lock()
                .map_err(|_| anyhow::anyhow!("Rust-native browser session lock poisoned"))?;

            let output = state.execute_action(
                action,
                self.native_headless,
                self.native_chrome_path.as_deref(),
            )?;

            Ok(ToolResult {
                success: true,
                output: serde_json::to_string_pretty(&output).unwrap_or_default(),
                error: None,
            })
        }

        #[cfg(not(feature = "browser-native"))]
        {
            let _ = action;
            anyhow::bail!(
                "Rust-native browser backend is not compiled. Rebuild with --features browser-native"
            )
        }
    }

    async fn execute_action(
        &self,
        action: BrowserAction,
        backend: ResolvedBackend,
    ) -> anyhow::Result<ToolResult> {
        match backend {
            ResolvedBackend::AgentBrowser => self.execute_agent_browser_action(action).await,
            ResolvedBackend::RustNative => self.execute_rust_native_action(action),
        }
    }

    #[allow(clippy::unnecessary_wraps, clippy::unused_self)]
    fn to_result(&self, resp: AgentBrowserResponse) -> anyhow::Result<ToolResult> {
        if resp.success {
            let output = resp
                .data
                .map(|d| serde_json::to_string_pretty(&d).unwrap_or_default())
                .unwrap_or_default();
            Ok(ToolResult {
                success: true,
                output,
                error: None,
            })
        } else {
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: resp.error,
            })
        }
    }
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Web browser automation with pluggable backends (agent-browser or rust-native). \
        Supports navigation, clicking, filling forms, screenshots, and page snapshots. \
        Use 'snapshot' to map interactive elements to refs (@e1, @e2), then use refs for \
        precise interaction. Enforces browser.allowed_domains for open actions."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["open", "snapshot", "click", "fill", "type", "get_text",
                             "get_title", "get_url", "screenshot", "wait", "press",
                             "hover", "scroll", "is_visible", "close", "find"],
                    "description": "Browser action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL to navigate to (for 'open' action)"
                },
                "selector": {
                    "type": "string",
                    "description": "Element selector: @ref (e.g. @e1), CSS (#id, .class), or text=..."
                },
                "value": {
                    "type": "string",
                    "description": "Value to fill or type"
                },
                "text": {
                    "type": "string",
                    "description": "Text to type or wait for"
                },
                "key": {
                    "type": "string",
                    "description": "Key to press (Enter, Tab, Escape, etc.)"
                },
                "direction": {
                    "type": "string",
                    "enum": ["up", "down", "left", "right"],
                    "description": "Scroll direction"
                },
                "pixels": {
                    "type": "integer",
                    "description": "Pixels to scroll"
                },
                "interactive_only": {
                    "type": "boolean",
                    "description": "For snapshot: only show interactive elements"
                },
                "compact": {
                    "type": "boolean",
                    "description": "For snapshot: remove empty structural elements"
                },
                "depth": {
                    "type": "integer",
                    "description": "For snapshot: limit tree depth"
                },
                "full_page": {
                    "type": "boolean",
                    "description": "For screenshot: capture full page"
                },
                "path": {
                    "type": "string",
                    "description": "File path for screenshot"
                },
                "ms": {
                    "type": "integer",
                    "description": "Milliseconds to wait"
                },
                "by": {
                    "type": "string",
                    "enum": ["role", "text", "label", "placeholder", "testid"],
                    "description": "For find: semantic locator type"
                },
                "find_action": {
                    "type": "string",
                    "enum": ["click", "fill", "text", "hover", "check"],
                    "description": "For find: action to perform on found element"
                },
                "fill_value": {
                    "type": "string",
                    "description": "For find with fill action: value to fill"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        // Security checks
        if !self.security.can_act() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Action blocked: autonomy is read-only".into()),
            });
        }

        if !self.security.record_action() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Action blocked: rate limit exceeded".into()),
            });
        }

        let backend = match self.resolve_backend().await {
            Ok(selected) => selected,
            Err(error) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(error.to_string()),
                });
            }
        };

        // Parse action from args
        let action_str = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'action' parameter"))?;

        let action = match action_str {
            "open" => {
                let url = args
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'url' for open action"))?;
                BrowserAction::Open { url: url.into() }
            }
            "snapshot" => BrowserAction::Snapshot {
                interactive_only: args
                    .get("interactive_only")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true), // Default to interactive for AI
                compact: args
                    .get("compact")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                depth: args
                    .get("depth")
                    .and_then(serde_json::Value::as_u64)
                    .map(|d| u32::try_from(d).unwrap_or(u32::MAX)),
            },
            "click" => {
                let selector = args
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'selector' for click"))?;
                BrowserAction::Click {
                    selector: selector.into(),
                }
            }
            "fill" => {
                let selector = args
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'selector' for fill"))?;
                let value = args
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'value' for fill"))?;
                BrowserAction::Fill {
                    selector: selector.into(),
                    value: value.into(),
                }
            }
            "type" => {
                let selector = args
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'selector' for type"))?;
                let text = args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'text' for type"))?;
                BrowserAction::Type {
                    selector: selector.into(),
                    text: text.into(),
                }
            }
            "get_text" => {
                let selector = args
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'selector' for get_text"))?;
                BrowserAction::GetText {
                    selector: selector.into(),
                }
            }
            "get_title" => BrowserAction::GetTitle,
            "get_url" => BrowserAction::GetUrl,
            "screenshot" => BrowserAction::Screenshot {
                path: args.get("path").and_then(|v| v.as_str()).map(String::from),
                full_page: args
                    .get("full_page")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            },
            "wait" => BrowserAction::Wait {
                selector: args
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                ms: args.get("ms").and_then(serde_json::Value::as_u64),
                text: args.get("text").and_then(|v| v.as_str()).map(String::from),
            },
            "press" => {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'key' for press"))?;
                BrowserAction::Press { key: key.into() }
            }
            "hover" => {
                let selector = args
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'selector' for hover"))?;
                BrowserAction::Hover {
                    selector: selector.into(),
                }
            }
            "scroll" => {
                let direction = args
                    .get("direction")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'direction' for scroll"))?;
                BrowserAction::Scroll {
                    direction: direction.into(),
                    pixels: args
                        .get("pixels")
                        .and_then(serde_json::Value::as_u64)
                        .map(|p| u32::try_from(p).unwrap_or(u32::MAX)),
                }
            }
            "is_visible" => {
                let selector = args
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'selector' for is_visible"))?;
                BrowserAction::IsVisible {
                    selector: selector.into(),
                }
            }
            "close" => BrowserAction::Close,
            "find" => {
                let by = args
                    .get("by")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'by' for find"))?;
                let value = args
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'value' for find"))?;
                let action = args
                    .get("find_action")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'find_action' for find"))?;
                BrowserAction::Find {
                    by: by.into(),
                    value: value.into(),
                    action: action.into(),
                    fill_value: args
                        .get("fill_value")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                }
            }
            _ => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Unknown action: {action_str}")),
                });
            }
        };

        self.execute_action(action, backend).await
    }
}

#[cfg(feature = "browser-native")]
mod native_backend {
    use super::BrowserAction;
    use anyhow::{Context, Result};
    use base64::Engine;
    use headless_chrome::{
        protocol::cdp::Page::CaptureScreenshotFormatOption, Browser, Element, LaunchOptions,
        LaunchOptionsBuilder, Tab,
    };
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Default)]
    pub struct NativeBrowserState {
        browser: Option<Browser>,
        tab: Option<Arc<Tab>>,
    }

    impl NativeBrowserState {
        pub fn is_available(headless: bool, chrome_path: Option<&str>) -> bool {
            launch_options(headless, chrome_path).is_ok()
        }

        #[allow(clippy::too_many_lines)]
        pub fn execute_action(
            &mut self,
            action: BrowserAction,
            headless: bool,
            chrome_path: Option<&str>,
        ) -> Result<Value> {
            match action {
                BrowserAction::Open { url } => {
                    let tab = self.ensure_session(headless, chrome_path)?;
                    tab.navigate_to(&url)
                        .with_context(|| format!("Failed to open URL: {url}"))?;
                    tab.wait_until_navigated()
                        .context("Navigation did not complete")?;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "open",
                        "url": tab.get_url(),
                    }))
                }
                BrowserAction::Snapshot {
                    interactive_only,
                    compact,
                    depth,
                } => {
                    let tab = self.active_tab()?;
                    let snapshot = evaluate_json(
                        tab,
                        &snapshot_script(interactive_only, compact, depth.map(i64::from)),
                    )?;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "snapshot",
                        "data": snapshot,
                    }))
                }
                BrowserAction::Click { selector } => {
                    let tab = self.active_tab()?;
                    find_element(tab, &selector)?.click()?;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "click",
                        "selector": selector,
                    }))
                }
                BrowserAction::Fill { selector, value } => {
                    let tab = self.active_tab()?;
                    let element = find_element(tab, &selector)?;
                    let _ = element.call_js_fn(
                        "function () { if ('value' in this) { this.value = ''; } }",
                        vec![],
                        false,
                    );
                    element.type_into(&value)?;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "fill",
                        "selector": selector,
                    }))
                }
                BrowserAction::Type { selector, text } => {
                    let tab = self.active_tab()?;
                    find_element(tab, &selector)?.type_into(&text)?;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "type",
                        "selector": selector,
                        "typed": text.len(),
                    }))
                }
                BrowserAction::GetText { selector } => {
                    let tab = self.active_tab()?;
                    let text = find_element(tab, &selector)?.get_inner_text()?;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "get_text",
                        "selector": selector,
                        "text": text,
                    }))
                }
                BrowserAction::GetTitle => {
                    let tab = self.active_tab()?;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "get_title",
                        "title": tab.get_title()?,
                    }))
                }
                BrowserAction::GetUrl => {
                    let tab = self.active_tab()?;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "get_url",
                        "url": tab.get_url(),
                    }))
                }
                BrowserAction::Screenshot { path, full_page } => {
                    let tab = self.active_tab()?;
                    let png = tab.capture_screenshot(
                        CaptureScreenshotFormatOption::Png,
                        None,
                        None,
                        full_page,
                    )?;

                    let mut payload = json!({
                        "backend": "rust_native",
                        "action": "screenshot",
                        "bytes": png.len(),
                        "full_page": full_page,
                    });

                    if let Some(path_str) = path {
                        std::fs::write(&path_str, &png)
                            .with_context(|| format!("Failed to write screenshot to {path_str}"))?;
                        payload["path"] = Value::String(path_str);
                    } else {
                        payload["png_base64"] =
                            Value::String(base64::engine::general_purpose::STANDARD.encode(&png));
                    }

                    Ok(payload)
                }
                BrowserAction::Wait { selector, ms, text } => {
                    let tab = self.active_tab()?;
                    if let Some(sel) = selector.as_ref() {
                        wait_for_selector(tab, sel)?;
                        Ok(json!({
                            "backend": "rust_native",
                            "action": "wait",
                            "selector": sel,
                        }))
                    } else if let Some(duration_ms) = ms {
                        std::thread::sleep(Duration::from_millis(duration_ms));
                        Ok(json!({
                            "backend": "rust_native",
                            "action": "wait",
                            "ms": duration_ms,
                        }))
                    } else if let Some(needle) = text.as_ref() {
                        let xpath = xpath_contains_text(needle);
                        tab.wait_for_xpath(&xpath)?;
                        Ok(json!({
                            "backend": "rust_native",
                            "action": "wait",
                            "text": needle,
                        }))
                    } else {
                        std::thread::sleep(Duration::from_millis(250));
                        Ok(json!({
                            "backend": "rust_native",
                            "action": "wait",
                            "ms": 250,
                        }))
                    }
                }
                BrowserAction::Press { key } => {
                    let tab = self.active_tab()?;
                    tab.press_key(&key)?;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "press",
                        "key": key,
                    }))
                }
                BrowserAction::Hover { selector } => {
                    let tab = self.active_tab()?;
                    find_element(tab, &selector)?.move_mouse_over()?;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "hover",
                        "selector": selector,
                    }))
                }
                BrowserAction::Scroll { direction, pixels } => {
                    let tab = self.active_tab()?;
                    let amount = i64::from(pixels.unwrap_or(600));
                    let (dx, dy) = match direction.as_str() {
                        "up" => (0, -amount),
                        "down" => (0, amount),
                        "left" => (-amount, 0),
                        "right" => (amount, 0),
                        _ => anyhow::bail!(
                            "Unsupported scroll direction '{direction}'. Use up/down/left/right"
                        ),
                    };

                    let position = evaluate_json(
                        tab,
                        &format!(
                            "window.scrollBy({dx}, {dy}); ({{ x: window.scrollX, y: window.scrollY }});"
                        ),
                    )?;

                    Ok(json!({
                        "backend": "rust_native",
                        "action": "scroll",
                        "position": position,
                    }))
                }
                BrowserAction::IsVisible { selector } => {
                    let tab = self.active_tab()?;
                    let visible = find_element(tab, &selector)?.is_visible()?;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "is_visible",
                        "selector": selector,
                        "visible": visible,
                    }))
                }
                BrowserAction::Close => {
                    self.tab = None;
                    self.browser = None;
                    Ok(json!({
                        "backend": "rust_native",
                        "action": "close",
                        "closed": true,
                    }))
                }
                BrowserAction::Find {
                    by,
                    value,
                    action,
                    fill_value,
                } => {
                    let tab = self.active_tab()?;
                    let selector = selector_for_find(&by, &value);
                    let payload = match action.as_str() {
                        "click" => {
                            find_element(tab, &selector)?.click()?;
                            json!({"result": "clicked"})
                        }
                        "fill" => {
                            let fill = fill_value.ok_or_else(|| {
                                anyhow::anyhow!("find_action='fill' requires fill_value")
                            })?;
                            let element = find_element(tab, &selector)?;
                            let _ = element.call_js_fn(
                                "function () { if ('value' in this) { this.value = ''; } }",
                                vec![],
                                false,
                            );
                            element.type_into(&fill)?;
                            json!({"result": "filled", "typed": fill.len()})
                        }
                        "text" => {
                            let text = find_element(tab, &selector)?.get_inner_text()?;
                            json!({"result": "text", "text": text})
                        }
                        "hover" => {
                            find_element(tab, &selector)?.move_mouse_over()?;
                            json!({"result": "hovered"})
                        }
                        "check" => {
                            let element = find_element(tab, &selector)?;
                            let checked_before = element_checked(&element)?;
                            if !checked_before {
                                element.click()?;
                            }
                            let checked_after = element_checked(&element)?;
                            json!({
                                "result": "checked",
                                "checked_before": checked_before,
                                "checked_after": checked_after,
                            })
                        }
                        _ => anyhow::bail!(
                            "Unsupported find_action '{action}'. Use click/fill/text/hover/check"
                        ),
                    };

                    Ok(json!({
                        "backend": "rust_native",
                        "action": "find",
                        "by": by,
                        "value": value,
                        "selector": selector,
                        "data": payload,
                    }))
                }
            }
        }

        fn ensure_session(
            &mut self,
            headless: bool,
            chrome_path: Option<&str>,
        ) -> Result<&Arc<Tab>> {
            if self.tab.is_none() {
                let options = launch_options(headless, chrome_path)?;
                let browser = Browser::new(options)
                    .context("Failed to initialize rust-native browser backend")?;
                let tab = browser
                    .new_tab()
                    .context("Failed to create browser tab for rust-native backend")?;

                self.browser = Some(browser);
                self.tab = Some(tab);
            }

            self.active_tab()
        }

        fn active_tab(&self) -> Result<&Arc<Tab>> {
            self.tab.as_ref().ok_or_else(|| {
                anyhow::anyhow!("No active native browser session. Run browser action='open' first")
            })
        }
    }

    fn launch_options(headless: bool, chrome_path: Option<&str>) -> Result<LaunchOptions<'static>> {
        let mut builder = LaunchOptionsBuilder::default();
        builder.headless(headless);

        if let Some(path) = chrome_path {
            builder.path(Some(PathBuf::from(path)));
        }

        builder.build().map_err(|error| {
            anyhow::anyhow!("Unable to build native browser launch options: {error}")
        })
    }

    fn evaluate_json(tab: &Arc<Tab>, script: &str) -> Result<Value> {
        let result = tab
            .evaluate(script, true)
            .context("Failed to evaluate JavaScript in browser tab")?;
        Ok(result.value.unwrap_or(Value::Null))
    }

    fn selector_for_find(by: &str, value: &str) -> String {
        let escaped = css_attr_escape(value);
        match by {
            "role" => format!(r#"[role=\"{escaped}\"]"#),
            "label" => format!("label={value}"),
            "placeholder" => format!(r#"[placeholder=\"{escaped}\"]"#),
            "testid" => format!(r#"[data-testid=\"{escaped}\"]"#),
            _ => format!("text={value}"),
        }
    }

    fn wait_for_selector(tab: &Arc<Tab>, selector: &str) -> Result<()> {
        match parse_selector(selector) {
            SelectorKind::Css(css) => {
                tab.wait_for_element(&css)?;
            }
            SelectorKind::XPath(xpath) => {
                tab.wait_for_xpath(&xpath)?;
            }
        }
        Ok(())
    }

    fn find_element<'a>(tab: &'a Arc<Tab>, selector: &str) -> Result<Element<'a>> {
        match parse_selector(selector) {
            SelectorKind::Css(css) => Ok(tab.wait_for_element(&css)?),
            SelectorKind::XPath(xpath) => Ok(tab.wait_for_xpath(&xpath)?),
        }
    }

    fn element_checked(element: &Element<'_>) -> Result<bool> {
        let checked = element
            .call_js_fn("function () { return !!this.checked; }", vec![], true)?
            .value
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        Ok(checked)
    }

    enum SelectorKind {
        Css(String),
        XPath(String),
    }

    fn parse_selector(selector: &str) -> SelectorKind {
        let trimmed = selector.trim();
        if let Some(text_query) = trimmed.strip_prefix("text=") {
            return SelectorKind::XPath(xpath_contains_text(text_query));
        }

        if let Some(label_query) = trimmed.strip_prefix("label=") {
            return SelectorKind::XPath(format!(
                "//label[contains(normalize-space(.), {})]",
                xpath_literal(label_query)
            ));
        }

        if trimmed.starts_with('@') {
            let escaped = css_attr_escape(trimmed);
            return SelectorKind::Css(format!(r#"[data-zc-ref=\"{escaped}\"]"#));
        }

        SelectorKind::Css(trimmed.to_string())
    }

    fn css_attr_escape(input: &str) -> String {
        input
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', " ")
    }

    fn xpath_contains_text(text: &str) -> String {
        format!("//*[contains(normalize-space(.), {})]", xpath_literal(text))
    }

    fn xpath_literal(input: &str) -> String {
        if !input.contains('"') {
            return format!("\"{input}\"");
        }
        if !input.contains('\'') {
            return format!("'{input}'");
        }

        let mut parts: Vec<String> = Vec::new();
        for (index, part) in input.split('"').enumerate() {
            if !part.is_empty() {
                parts.push(format!("\"{part}\""));
            }
            if index + 1 != input.matches('"').count() + 1 {
                parts.push("'\"'".to_string());
            }
        }

        if parts.is_empty() {
            "\"\"".to_string()
        } else {
            format!("concat({})", parts.join(","))
        }
    }

    fn snapshot_script(interactive_only: bool, compact: bool, depth: Option<i64>) -> String {
        let depth_literal = depth
            .map(|level| level.to_string())
            .unwrap_or_else(|| "null".to_string());

        format!(
            r#"(() => {{
  const interactiveOnly = {interactive_only};
  const compact = {compact};
  const maxDepth = {depth_literal};
  const nodes = [];
  const root = document.body || document.documentElement;
  let counter = 0;

  const isVisible = (el) => {{
    const style = window.getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity || 1) === 0) {{
      return false;
    }}
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  }};

  const isInteractive = (el) => {{
    if (el.matches('a,button,input,select,textarea,summary,[role],*[tabindex]')) return true;
    return typeof el.onclick === 'function';
  }};

  const describe = (el, depth) => {{
    const interactive = isInteractive(el);
    const text = (el.innerText || el.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 140);
    if (interactiveOnly && !interactive) return;
    if (compact && !interactive && !text) return;

    const ref = '@e' + (++counter);
    el.setAttribute('data-zc-ref', ref);
    nodes.push({{
      ref,
      depth,
      tag: el.tagName.toLowerCase(),
      id: el.id || null,
      role: el.getAttribute('role'),
      text,
      interactive,
    }});
  }};

  const walk = (el, depth) => {{
    if (!(el instanceof Element)) return;
    if (maxDepth !== null && depth > maxDepth) return;
    if (isVisible(el)) {{
      describe(el, depth);
    }}
    for (const child of el.children) {{
      walk(child, depth + 1);
      if (nodes.length >= 400) return;
    }}
  }};

  if (root) walk(root, 0);

  return {{
    title: document.title,
    url: window.location.href,
    count: nodes.length,
    nodes,
  }};
}})();"#
        )
    }
}

// ── Helper functions ─────────────────────────────────────────────

fn normalize_domains(domains: Vec<String>) -> Vec<String> {
    domains
        .into_iter()
        .map(|d| d.trim().to_lowercase())
        .filter(|d| !d.is_empty())
        .collect()
}

fn extract_host(url_str: &str) -> anyhow::Result<String> {
    // Simple host extraction without url crate
    let url = url_str.trim();
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("file://"))
        .unwrap_or(url);

    // Extract host — handle bracketed IPv6 addresses like [::1]:8080
    let authority = without_scheme.split('/').next().unwrap_or(without_scheme);

    let host = if authority.starts_with('[') {
        // IPv6: take everything up to and including the closing ']'
        authority.find(']').map_or(authority, |i| &authority[..=i])
    } else {
        // IPv4 or hostname: take everything before the port separator
        authority.split(':').next().unwrap_or(authority)
    };

    if host.is_empty() {
        anyhow::bail!("Invalid URL: no host");
    }

    Ok(host.to_lowercase())
}

fn is_private_host(host: &str) -> bool {
    // Strip brackets from IPv6 addresses like [::1]
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);

    if bare == "localhost" {
        return true;
    }

    // Parse as IP address to catch all representations (decimal, hex, octal, mapped)
    if let Ok(ip) = bare.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_broadcast()
            }
            std::net::IpAddr::V6(v6) => {
                let segs = v6.segments();
                v6.is_loopback()
                    || v6.is_unspecified()
                    // Unique-local (fc00::/7) — IPv6 equivalent of RFC 1918
                    || (segs[0] & 0xfe00) == 0xfc00
                    // Link-local (fe80::/10)
                    || (segs[0] & 0xffc0) == 0xfe80
                    // IPv4-mapped addresses (::ffff:127.0.0.1)
                    || v6.to_ipv4_mapped().is_some_and(|v4| {
                        v4.is_loopback()
                            || v4.is_private()
                            || v4.is_link_local()
                            || v4.is_unspecified()
                            || v4.is_broadcast()
                    })
            }
        };
    }

    // Fallback string patterns for hostnames that look like IPs but don't parse
    // (e.g., partial addresses used in DNS names).
    let string_patterns = [
        "127.", "10.", "192.168.", "0.0.0.0", "172.16.", "172.17.", "172.18.", "172.19.",
        "172.20.", "172.21.", "172.22.", "172.23.", "172.24.", "172.25.", "172.26.", "172.27.",
        "172.28.", "172.29.", "172.30.", "172.31.",
    ];

    string_patterns.iter().any(|p| bare.starts_with(p))
}

fn host_matches_allowlist(host: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|pattern| {
        if pattern == "*" {
            return true;
        }
        if pattern.starts_with("*.") {
            // Wildcard subdomain match
            let suffix = &pattern[1..]; // ".example.com"
            host.ends_with(suffix) || host == &pattern[2..]
        } else {
            // Exact match or subdomain
            host == pattern || host.ends_with(&format!(".{pattern}"))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_domains_works() {
        let domains = vec![
            "  Example.COM  ".into(),
            "docs.example.com".into(),
            String::new(),
        ];
        let normalized = normalize_domains(domains);
        assert_eq!(normalized, vec!["example.com", "docs.example.com"]);
    }

    #[test]
    fn extract_host_works() {
        assert_eq!(
            extract_host("https://example.com/path").unwrap(),
            "example.com"
        );
        assert_eq!(
            extract_host("https://Sub.Example.COM:8080/").unwrap(),
            "sub.example.com"
        );
    }

    #[test]
    fn extract_host_handles_ipv6() {
        // IPv6 with brackets (required for URLs with ports)
        assert_eq!(extract_host("https://[::1]/path").unwrap(), "[::1]");
        // IPv6 with brackets and port
        assert_eq!(
            extract_host("https://[2001:db8::1]:8080/path").unwrap(),
            "[2001:db8::1]"
        );
        // IPv6 with brackets, trailing slash
        assert_eq!(extract_host("https://[fe80::1]/").unwrap(), "[fe80::1]");
    }

    #[test]
    fn is_private_host_detects_local() {
        assert!(is_private_host("localhost"));
        assert!(is_private_host("127.0.0.1"));
        assert!(is_private_host("192.168.1.1"));
        assert!(is_private_host("10.0.0.1"));
        assert!(!is_private_host("example.com"));
        assert!(!is_private_host("google.com"));
    }

    #[test]
    fn is_private_host_catches_ipv6() {
        assert!(is_private_host("::1"));
        assert!(is_private_host("[::1]"));
        assert!(is_private_host("0.0.0.0"));
    }

    #[test]
    fn is_private_host_catches_mapped_ipv4() {
        // IPv4-mapped IPv6 addresses
        assert!(is_private_host("::ffff:127.0.0.1"));
        assert!(is_private_host("::ffff:10.0.0.1"));
        assert!(is_private_host("::ffff:192.168.1.1"));
    }

    #[test]
    fn is_private_host_catches_ipv6_private_ranges() {
        // Unique-local (fc00::/7)
        assert!(is_private_host("fd00::1"));
        assert!(is_private_host("fc00::1"));
        // Link-local (fe80::/10)
        assert!(is_private_host("fe80::1"));
        // Public IPv6 should pass
        assert!(!is_private_host("2001:db8::1"));
    }

    #[test]
    fn validate_url_blocks_ipv6_ssrf() {
        let security = Arc::new(SecurityPolicy::default());
        let tool = BrowserTool::new(security, vec!["*".into()], None);
        assert!(tool.validate_url("https://[::1]/").is_err());
        assert!(tool.validate_url("https://[::ffff:127.0.0.1]/").is_err());
        assert!(tool
            .validate_url("https://[::ffff:10.0.0.1]:8080/")
            .is_err());
    }

    #[test]
    fn host_matches_allowlist_exact() {
        let allowed = vec!["example.com".into()];
        assert!(host_matches_allowlist("example.com", &allowed));
        assert!(host_matches_allowlist("sub.example.com", &allowed));
        assert!(!host_matches_allowlist("notexample.com", &allowed));
    }

    #[test]
    fn host_matches_allowlist_wildcard() {
        let allowed = vec!["*.example.com".into()];
        assert!(host_matches_allowlist("sub.example.com", &allowed));
        assert!(host_matches_allowlist("example.com", &allowed));
        assert!(!host_matches_allowlist("other.com", &allowed));
    }

    #[test]
    fn host_matches_allowlist_star() {
        let allowed = vec!["*".into()];
        assert!(host_matches_allowlist("anything.com", &allowed));
        assert!(host_matches_allowlist("example.org", &allowed));
    }

    #[test]
    fn browser_backend_parser_accepts_supported_values() {
        assert_eq!(
            BrowserBackendKind::parse("agent_browser").unwrap(),
            BrowserBackendKind::AgentBrowser
        );
        assert_eq!(
            BrowserBackendKind::parse("rust-native").unwrap(),
            BrowserBackendKind::RustNative
        );
        assert_eq!(
            BrowserBackendKind::parse("auto").unwrap(),
            BrowserBackendKind::Auto
        );
    }

    #[test]
    fn browser_backend_parser_rejects_unknown_values() {
        assert!(BrowserBackendKind::parse("playwright").is_err());
    }

    #[test]
    fn browser_tool_default_backend_is_agent_browser() {
        let security = Arc::new(SecurityPolicy::default());
        let tool = BrowserTool::new(security, vec!["example.com".into()], None);
        assert_eq!(
            tool.configured_backend().unwrap(),
            BrowserBackendKind::AgentBrowser
        );
    }

    #[test]
    fn browser_tool_accepts_auto_backend_config() {
        let security = Arc::new(SecurityPolicy::default());
        let tool = BrowserTool::new_with_backend(
            security,
            vec!["example.com".into()],
            None,
            "auto".into(),
            true,
            None,
        );
        assert_eq!(tool.configured_backend().unwrap(), BrowserBackendKind::Auto);
    }

    #[test]
    fn browser_tool_name() {
        let security = Arc::new(SecurityPolicy::default());
        let tool = BrowserTool::new(security, vec!["example.com".into()], None);
        assert_eq!(tool.name(), "browser");
    }

    #[test]
    fn browser_tool_validates_url() {
        let security = Arc::new(SecurityPolicy::default());
        let tool = BrowserTool::new(security, vec!["example.com".into()], None);

        // Valid
        assert!(tool.validate_url("https://example.com").is_ok());
        assert!(tool.validate_url("https://sub.example.com/path").is_ok());

        // Invalid - not in allowlist
        assert!(tool.validate_url("https://other.com").is_err());

        // Invalid - private host
        assert!(tool.validate_url("https://localhost").is_err());
        assert!(tool.validate_url("https://127.0.0.1").is_err());

        // Invalid - not https
        assert!(tool.validate_url("ftp://example.com").is_err());

        // File URLs allowed
        assert!(tool.validate_url("file:///tmp/test.html").is_ok());
    }

    #[test]
    fn browser_tool_empty_allowlist_blocks() {
        let security = Arc::new(SecurityPolicy::default());
        let tool = BrowserTool::new(security, vec![], None);
        assert!(tool.validate_url("https://example.com").is_err());
    }
}
