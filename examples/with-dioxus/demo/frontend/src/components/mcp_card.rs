use dioxus::prelude::*;

#[component]
pub fn McpCard(api_url: String) -> Element {
    rsx! {
        section { class: "card mcp-card",
            h2 {
                "MCP Tools "
                span { class: "badge green", "model context protocol" }
            }

            p { class: "mcp-desc",
                "This demo exposes MCP tools with OAuth 2.1 authentication. AI assistants authenticate via browser login and can act on behalf of the user."
            }

            div { class: "code-block",
                div { class: "code-label", "CLAUDE CODE" }
                pre {
                    code { "claude mcp add forge-demo --transport http {api_url}/_api/mcp" }
                }
            }

            div { class: "mcp-tools",
                div { class: "mcp-tool",
                    div { class: "tool-header",
                        span { class: "tool-name mono", "demo.me" }
                        span { class: "tool-badge", "authenticated" }
                    }
                    span { class: "tool-desc", "Get your own profile (requires OAuth login)" }
                }
                div { class: "mcp-tool",
                    div { class: "tool-header",
                        span { class: "tool-name mono", "demo.list_users" }
                        span { class: "tool-badge", "public" }
                    }
                    span { class: "tool-desc", "List all users with their roles" }
                }
                div { class: "mcp-tool",
                    div { class: "tool-header",
                        span { class: "tool-name mono", "demo.get_user_by_email" }
                        span { class: "tool-badge", "public" }
                    }
                    span { class: "tool-desc", "Look up a single user by email address" }
                }
            }
        }
    }
}
