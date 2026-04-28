use dioxus::prelude::*;

use crate::forge::use_get_trades_subscription;

#[component]
pub fn TradesCard() -> Element {
    let state = use_get_trades_subscription();
    let trades = state.data.clone().unwrap_or_default();

    rsx! {
        section { class: "card",
            h2 { "Live Trades " span { class: "badge green", "daemon + websocket" } }
            table { class: "trades-table",
                thead {
                    tr { th { "Symbol" } th { "Price" } th { "Qty" } th { "Side" } }
                }
                tbody {
                    if trades.is_empty() {
                        for i in 0..4 {
                            tr { key: "{i}", class: "placeholder-row",
                                td { class: "mono", "---" }
                                td { class: "mono", "-.-----" }
                                td { class: "mono", "-.----" }
                                td { "-" }
                            }
                        }
                    } else {
                        for trade in &trades {
                            tr { key: "{trade.id}",
                                td { class: "mono", "{trade.symbol}" }
                                td { class: "mono", "{trade.price:.5}" }
                                td { class: "mono", "{trade.quantity:.4}" }
                                td {
                                    class: if trade.is_buyer_maker { "sell" } else { "buy" },
                                    if trade.is_buyer_maker { "SELL" } else { "BUY" }
                                }
                            }
                        }
                    }
                }
            }
            if trades.is_empty() {
                p { class: "muted small", "Connecting to Binance WebSocket..." }
            } else {
                p { class: "muted small", "Streaming from Binance EUR/USDT" }
            }
        }
    }
}
