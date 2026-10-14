//! Business XML stanza nodes for WhatsApp Interactive Messages and Native Flows.
//!
//! WhatsApp Web Multi-Device protocol requires outbound `<message>` stanzas carrying
//! interactive elements (Native Flow buttons, List menus, Carousel cards) to include
//! stanza-level `<biz>` and `<bot>` child nodes. Without these XML nodes, the WhatsApp
//! server will acknowledge the stanza (`server_ack`), but official Android/iOS clients
//! will silently discard rendering.

use baileys_binary::BinaryNode;
use baileys_proto::proto::Message;

/// Type of business interactive payload to advertise in the stanza `<biz>` node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BizKind {
    /// Modern Native Flow buttons, carousels, or legacy button messages.
    NativeFlow,
    /// Interactive List menu messages.
    List,
    /// Dedicated single-action button (e.g. "send_location", "cta_catalog", "address_message").
    SpecialButton(String),
}

/// Unwraps common message wrappers (view_once, ephemeral, etc.) to get the inner payload.
pub fn unwrap_message_wrappers(mut msg: &Message) -> &Message {
    loop {
        if let Some(ref vo) = msg.view_once_message {
            if let Some(ref inner) = vo.message {
                msg = inner;
                continue;
            }
        }
        if let Some(ref vo2) = msg.view_once_message_v2 {
            if let Some(ref inner) = vo2.message {
                msg = inner;
                continue;
            }
        }
        if let Some(ref vo_ext) = msg.view_once_message_v2_extension {
            if let Some(ref inner) = vo_ext.message {
                msg = inner;
                continue;
            }
        }
        if let Some(ref eph) = msg.ephemeral_message {
            if let Some(ref inner) = eph.message {
                msg = inner;
                continue;
            }
        }
        if let Some(ref dwc) = msg.document_with_caption_message {
            if let Some(ref inner) = dwc.message {
                msg = inner;
                continue;
            }
        }
        break;
    }
    msg
}

/// Detects whether a `Message` requires a `<biz>` node, and determines the appropriate `BizKind`.
pub fn detect_biz_kind(msg: &Message) -> Option<BizKind> {
    let payload = unwrap_message_wrappers(msg);

    if payload.list_message.is_some() {
        return Some(BizKind::List);
    }

    if payload.buttons_message.is_some() {
        return Some(BizKind::NativeFlow);
    }

    if let Some(ref interactive) = payload.interactive_message {
        if let Some(ref inner_inter) = interactive.interactive_message {
            match inner_inter {
                baileys_proto::proto::message::interactive_message::InteractiveMessage::NativeFlowMessage(flow) => {
                    // Check if there is a single special button
                    if flow.buttons.len() == 1 {
                        if let Some(ref btn_name) = flow.buttons[0].name {
                            match btn_name.as_str() {
                                "send_location" | "cta_catalog" | "address_message" => {
                                    return Some(BizKind::SpecialButton(btn_name.clone()));
                                }
                                _ => {}
                            }
                        }
                    }
                    return Some(BizKind::NativeFlow);
                }
                baileys_proto::proto::message::interactive_message::InteractiveMessage::CarouselMessage(_) => {
                    return Some(BizKind::NativeFlow);
                }
                _ => return Some(BizKind::NativeFlow),
            }
        }
        return Some(BizKind::NativeFlow);
    }

    None
}

/// Builds the stanza-level `<biz>` XML node for the given `BizKind`.
pub fn build_biz_node(kind: &BizKind) -> BinaryNode {
    match kind {
        BizKind::NativeFlow => BinaryNode::new("biz").with_child(
            BinaryNode::new("interactive")
                .with_attr("type", "native_flow")
                .with_attr("v", "1")
                .with_child(
                    BinaryNode::new("native_flow")
                        .with_attr("v", "9")
                        .with_attr("name", "mixed"),
                ),
        ),
        BizKind::List => BinaryNode::new("biz").with_child(
            BinaryNode::new("list")
                .with_attr("type", "product_list")
                .with_attr("v", "2"),
        ),
        BizKind::SpecialButton(name) => BinaryNode::new("biz").with_child(
            BinaryNode::new("interactive")
                .with_attr("type", "native_flow")
                .with_attr("v", "1")
                .with_child(
                    BinaryNode::new("native_flow")
                        .with_attr("v", "2")
                        .with_attr("name", name.as_str()),
                ),
        ),
    }
}

/// Builds the stanza-level `<bot biz_bot="1"/>` node required for 1:1 user chats.
pub fn build_bot_node() -> BinaryNode {
    BinaryNode::new("bot").with_attr("biz_bot", "1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use baileys_proto::proto::message::interactive_message::native_flow_message::NativeFlowButton;
    use baileys_proto::proto::message::interactive_message::{Body, NativeFlowMessage};
    use baileys_proto::proto::message::{FutureProofMessage, InteractiveMessage, ListMessage};

    #[test]
    fn test_detect_biz_kind_native_flow() {
        let msg = Message {
            interactive_message: Some(Box::new(InteractiveMessage {
                body: Some(Body {
                    text: Some("Test".to_string()),
                }),
                interactive_message: Some(
                    baileys_proto::proto::message::interactive_message::InteractiveMessage::NativeFlowMessage(
                        NativeFlowMessage {
                            buttons: vec![NativeFlowButton {
                                name: Some("quick_reply".to_string()),
                                button_params_json: Some("{}".to_string()),
                            }],
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            })),
            ..Default::default()
        };

        assert_eq!(detect_biz_kind(&msg), Some(BizKind::NativeFlow));
        let node = build_biz_node(&BizKind::NativeFlow);
        assert_eq!(node.tag, "biz");
        let inter = node.get_child("interactive").expect("interactive node");
        assert_eq!(inter.get_attr("type"), Some("native_flow"));
        assert_eq!(inter.get_attr("v"), Some("1"));
        let nf = inter.get_child("native_flow").expect("native_flow node");
        assert_eq!(nf.get_attr("v"), Some("9"));
        assert_eq!(nf.get_attr("name"), Some("mixed"));
    }

    #[test]
    fn test_detect_biz_kind_wrapped_in_view_once() {
        let inner = Message {
            interactive_message: Some(Box::new(InteractiveMessage {
                interactive_message: Some(
                    baileys_proto::proto::message::interactive_message::InteractiveMessage::NativeFlowMessage(
                        NativeFlowMessage {
                            buttons: vec![
                                NativeFlowButton {
                                    name: Some("quick_reply".to_string()),
                                    button_params_json: Some("{}".to_string()),
                                },
                                NativeFlowButton {
                                    name: Some("cta_url".to_string()),
                                    button_params_json: Some("{}".to_string()),
                                },
                            ],
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            })),
            ..Default::default()
        };

        let wrapped = Message {
            view_once_message: Some(Box::new(FutureProofMessage {
                message: Some(Box::new(inner)),
            })),
            ..Default::default()
        };

        assert_eq!(detect_biz_kind(&wrapped), Some(BizKind::NativeFlow));
    }

    #[test]
    fn test_detect_biz_kind_special_button() {
        let msg = Message {
            interactive_message: Some(Box::new(InteractiveMessage {
                interactive_message: Some(
                    baileys_proto::proto::message::interactive_message::InteractiveMessage::NativeFlowMessage(
                        NativeFlowMessage {
                            buttons: vec![NativeFlowButton {
                                name: Some("send_location".to_string()),
                                button_params_json: Some("{}".to_string()),
                            }],
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            })),
            ..Default::default()
        };

        assert_eq!(
            detect_biz_kind(&msg),
            Some(BizKind::SpecialButton("send_location".to_string()))
        );
        let node = build_biz_node(&BizKind::SpecialButton("send_location".to_string()));
        let inter = node.get_child("interactive").expect("interactive node");
        let nf = inter.get_child("native_flow").expect("native_flow node");
        assert_eq!(nf.get_attr("v"), Some("2"));
        assert_eq!(nf.get_attr("name"), Some("send_location"));
    }

    #[test]
    fn test_detect_biz_kind_list() {
        let msg = Message {
            list_message: Some(Box::new(ListMessage::default())),
            ..Default::default()
        };

        assert_eq!(detect_biz_kind(&msg), Some(BizKind::List));
        let node = build_biz_node(&BizKind::List);
        assert_eq!(node.tag, "biz");
        let list = node.get_child("list").expect("list node");
        assert_eq!(list.get_attr("type"), Some("product_list"));
        assert_eq!(list.get_attr("v"), Some("2"));
    }

    #[test]
    fn test_build_bot_node() {
        let bot = build_bot_node();
        assert_eq!(bot.tag, "bot");
        assert_eq!(bot.get_attr("biz_bot"), Some("1"));
    }
}
