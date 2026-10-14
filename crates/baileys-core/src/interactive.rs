//! Type-safe builders for WhatsApp Interactive Messages, Native Flows, Carousel Cards, and List Messages.

use baileys_proto::proto::message::interactive_message::native_flow_message::NativeFlowButton;
use baileys_proto::proto::message::interactive_message::{
    Body, CarouselMessage, Footer, Header, NativeFlowMessage,
};
use baileys_proto::proto::message::InteractiveMessage;
use baileys_proto::proto::Message;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Header types supported by WhatsApp Interactive Messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractiveHeader {
    Text(String),
    Image {
        url: Option<String>,
        direct_path: Option<String>,
        media_key: Vec<u8>,
        file_sha256: Vec<u8>,
        file_enc_sha256: Vec<u8>,
        file_length: u64,
        jpeg_thumbnail: Option<Vec<u8>>,
    },
}

impl InteractiveHeader {
    pub fn into_proto(self) -> Header {
        match self {
            InteractiveHeader::Text(t) => Header {
                title: Some(t),
                has_media_attachment: Some(false),
                ..Default::default()
            },
            InteractiveHeader::Image {
                url,
                direct_path,
                media_key,
                file_sha256,
                file_enc_sha256,
                file_length,
                jpeg_thumbnail,
            } => {
                let img = baileys_proto::proto::message::ImageMessage {
                    url,
                    direct_path,
                    media_key: Some(media_key),
                    file_sha256: Some(file_sha256),
                    file_enc_sha256: Some(file_enc_sha256),
                    file_length: Some(file_length),
                    mimetype: Some("image/jpeg".to_string()),
                    jpeg_thumbnail,
                    ..Default::default()
                };
                Header {
                    has_media_attachment: Some(true),
                    media: Some(baileys_proto::proto::message::interactive_message::header::Media::ImageMessage(Box::new(img))),
                    ..Default::default()
                }
            }
        }
    }
}

/// A single card inside a Carousel Message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarouselCard {
    pub header: Option<InteractiveHeader>,
    pub body_text: String,
    pub footer_text: Option<String>,
    pub buttons: Vec<NativeFlowButton>,
}

impl CarouselCard {
    pub fn new(body_text: impl Into<String>) -> Self {
        Self {
            header: None,
            body_text: body_text.into(),
            footer_text: None,
            buttons: Vec::new(),
        }
    }

    pub fn header(mut self, header: InteractiveHeader) -> Self {
        self.header = Some(header);
        self
    }

    pub fn footer(mut self, footer: impl Into<String>) -> Self {
        self.footer_text = Some(footer.into());
        self
    }

    pub fn add_quick_reply(mut self, id: &str, display_text: &str) -> Self {
        let params = json!({
            "display_text": display_text,
            "id": id
        });
        self.buttons.push(NativeFlowButton {
            name: Some("quick_reply".to_string()),
            button_params_json: Some(params.to_string()),
        });
        self
    }

    pub fn add_url_button(mut self, display_text: &str, url: &str) -> Self {
        let params = json!({
            "display_text": display_text,
            "url": url,
            "merchant_url": url
        });
        self.buttons.push(NativeFlowButton {
            name: Some("cta_url".to_string()),
            button_params_json: Some(params.to_string()),
        });
        self
    }

    pub fn into_proto(self) -> InteractiveMessage {
        InteractiveMessage {
            header: self.header.map(|h| Box::new(h.into_proto())),
            body: Some(Body {
                text: Some(self.body_text),
            }),
            footer: self.footer_text.map(|f| Box::new(Footer {
                text: Some(f),
                ..Default::default()
            })),
            interactive_message: Some(
                baileys_proto::proto::message::interactive_message::InteractiveMessage::NativeFlowMessage(
                    NativeFlowMessage {
                        buttons: self.buttons,
                        message_version: Some(1),
                        ..Default::default()
                    },
                ),
            ),
            ..Default::default()
        }
    }
}

/// A row inside a Section for List Messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveListRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
}

/// A section grouping multiple rows in a List Message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveListSection {
    pub title: String,
    pub rows: Vec<InteractiveListRow>,
}

/// Type-safe builder for WhatsApp Interactive Messages (Native Flows & Carousels).
#[derive(Debug, Clone, Default)]
pub struct InteractiveMessageBuilder {
    body_text: String,
    footer_text: Option<String>,
    header: Option<InteractiveHeader>,
    buttons: Vec<NativeFlowButton>,
    carousel_cards: Vec<CarouselCard>,
}

impl InteractiveMessageBuilder {
    pub fn new(body: impl Into<String>) -> Self {
        Self {
            body_text: body.into(),
            footer_text: None,
            header: None,
            buttons: Vec::new(),
            carousel_cards: Vec::new(),
        }
    }

    pub fn header(mut self, header: InteractiveHeader) -> Self {
        self.header = Some(header);
        self
    }

    pub fn footer(mut self, footer: impl Into<String>) -> Self {
        self.footer_text = Some(footer.into());
        self
    }

    /// Adds a Quick Reply button (`quick_reply`).
    pub fn add_quick_reply(mut self, id: &str, display_text: &str) -> Self {
        let params = json!({
            "display_text": display_text,
            "id": id
        });
        self.buttons.push(NativeFlowButton {
            name: Some("quick_reply".to_string()),
            button_params_json: Some(params.to_string()),
        });
        self
    }

    /// Adds a Call-to-Action URL button (`cta_url`).
    pub fn add_url_button(mut self, display_text: &str, url: &str) -> Self {
        let params = json!({
            "display_text": display_text,
            "url": url,
            "merchant_url": url
        });
        self.buttons.push(NativeFlowButton {
            name: Some("cta_url".to_string()),
            button_params_json: Some(params.to_string()),
        });
        self
    }

    /// Adds a Call phone button (`cta_call`).
    pub fn add_call_button(mut self, display_text: &str, phone_number: &str) -> Self {
        let params = json!({
            "display_text": display_text,
            "phone_number": phone_number
        });
        self.buttons.push(NativeFlowButton {
            name: Some("cta_call".to_string()),
            button_params_json: Some(params.to_string()),
        });
        self
    }

    /// Adds a Copy to Clipboard button (`cta_copy`).
    pub fn add_copy_button(mut self, display_text: &str, copy_code: &str) -> Self {
        let params = json!({
            "display_text": display_text,
            "copy_code": copy_code
        });
        self.buttons.push(NativeFlowButton {
            name: Some("cta_copy".to_string()),
            button_params_json: Some(params.to_string()),
        });
        self
    }

    /// Adds an interactive List Menu button (`single_select`).
    pub fn add_list_menu(mut self, title: &str, sections: &[InteractiveListSection]) -> Self {
        let sec_json: Vec<serde_json::Value> = sections
            .iter()
            .map(|s| {
                let rows: Vec<serde_json::Value> = s
                    .rows
                    .iter()
                    .map(|r| {
                        json!({
                            "title": r.title,
                            "description": r.description.as_deref().unwrap_or(""),
                            "row_id": r.id
                        })
                    })
                    .collect();
                json!({
                    "title": s.title,
                    "rows": rows
                })
            })
            .collect();

        let params = json!({
            "title": title,
            "sections": sec_json
        });

        self.buttons.push(NativeFlowButton {
            name: Some("single_select".to_string()),
            button_params_json: Some(params.to_string()),
        });
        self
    }

    /// Adds a WhatsApp Native Flow button (`flow`).
    pub fn add_flow_button(
        mut self,
        id: &str,
        display_text: &str,
        flow_id: &str,
        flow_token: &str,
        flow_action: Option<&str>,
        flow_action_payload: Option<serde_json::Value>,
    ) -> Self {
        let mut params = json!({
            "id": id,
            "display_text": display_text,
            "flow_message_version": "3",
            "flow_token": flow_token,
            "flow_id": flow_id,
            "flow_action": flow_action.unwrap_or("navigate"),
        });

        if let Some(payload) = flow_action_payload {
            params["flow_action_payload"] = payload;
        }

        self.buttons.push(NativeFlowButton {
            name: Some("flow".to_string()),
            button_params_json: Some(params.to_string()),
        });
        self
    }

    /// Adds a carousel card to the message.
    pub fn add_carousel_card(mut self, card: CarouselCard) -> Self {
        self.carousel_cards.push(card);
        self
    }

    /// Builds the `Message` protobuf.
    pub fn build(self) -> Message {
        let interactive = if !self.carousel_cards.is_empty() {
            // Carousel Message
            let cards: Vec<InteractiveMessage> = self
                .carousel_cards
                .into_iter()
                .map(|c| c.into_proto())
                .collect();

            let carousel = CarouselMessage {
                cards,
                message_version: Some(1),
                carousel_card_type: Some(1), // HSCROLL_CARDS
            };

            InteractiveMessage {
                header: self.header.map(|h| Box::new(h.into_proto())),
                body: Some(Body {
                    text: Some(self.body_text),
                }),
                footer: self.footer_text.map(|f| Box::new(Footer {
                    text: Some(f),
                    ..Default::default()
                })),
                interactive_message: Some(
                    baileys_proto::proto::message::interactive_message::InteractiveMessage::CarouselMessage(
                        carousel,
                    ),
                ),
                ..Default::default()
            }
        } else {
            // Native Flow Message
            let flow = NativeFlowMessage {
                buttons: self.buttons,
                message_version: Some(1),
                ..Default::default()
            };

            InteractiveMessage {
                header: self.header.map(|h| Box::new(h.into_proto())),
                body: Some(Body {
                    text: Some(self.body_text),
                }),
                footer: self.footer_text.map(|f| Box::new(Footer {
                    text: Some(f),
                    ..Default::default()
                })),
                interactive_message: Some(
                    baileys_proto::proto::message::interactive_message::InteractiveMessage::NativeFlowMessage(
                        flow,
                    ),
                ),
                ..Default::default()
            }
        };

        wrap_interactive_view_once(interactive)
    }
}

/// Wraps an `InteractiveMessage` inside a `view_once_message` container with
/// `MessageContextInfo { device_list_metadata: Some(DeviceListMetadata::default()), device_list_metadata_version: Some(2) }`.
///
/// This specific encapsulation is strictly required by official WhatsApp Android and iOS
/// clients to recognize and render native flow interactive buttons and menus.
pub fn wrap_interactive_view_once(interactive: InteractiveMessage) -> Message {
    let inner = Message {
        message_context_info: Some(baileys_proto::proto::MessageContextInfo {
            device_list_metadata: Some(baileys_proto::proto::DeviceListMetadata::default()),
            device_list_metadata_version: Some(2),
            ..Default::default()
        }),
        interactive_message: Some(Box::new(interactive)),
        ..Default::default()
    };

    Message {
        view_once_message: Some(Box::new(
            baileys_proto::proto::message::FutureProofMessage {
                message: Some(Box::new(inner)),
            },
        )),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interactive_native_flow_quick_reply_and_url() {
        let msg = InteractiveMessageBuilder::new("Hello from Rose-Baileys!")
            .footer("Select an option below")
            .add_quick_reply("btn_1", "Confirm Booking")
            .add_url_button(
                "Visit Website",
                "https://github.com/Rose-Project/rose-baileys",
            )
            .add_call_button("Call Support", "+628123456789")
            .build();

        assert!(msg.view_once_message.is_some());
        let inner = msg.view_once_message.unwrap().message.unwrap();
        assert_eq!(
            inner
                .message_context_info
                .as_ref()
                .and_then(|c| c.device_list_metadata_version),
            Some(2)
        );
        assert!(inner.interactive_message.is_some());
        let interactive = inner.interactive_message.unwrap();

        assert_eq!(
            interactive.body.unwrap().text.as_deref(),
            Some("Hello from Rose-Baileys!")
        );
        assert_eq!(
            interactive.footer.unwrap().text.as_deref(),
            Some("Select an option below")
        );

        match interactive.interactive_message.unwrap() {
            baileys_proto::proto::message::interactive_message::InteractiveMessage::NativeFlowMessage(flow) => {
                assert_eq!(flow.buttons.len(), 3);
                assert_eq!(flow.buttons[0].name.as_deref(), Some("quick_reply"));
                assert_eq!(flow.buttons[1].name.as_deref(), Some("cta_url"));
                assert_eq!(flow.buttons[2].name.as_deref(), Some("cta_call"));
            }
            _ => panic!("Expected NativeFlowMessage"),
        }
    }

    #[test]
    fn test_interactive_carousel_message() {
        let card1 = CarouselCard::new("Product 1: Rose High-Performance Engine")
            .footer("$49.99")
            .add_url_button("Buy Now", "https://example.com/item1");

        let card2 = CarouselCard::new("Product 2: Rose Enterprise Cluster")
            .footer("$199.99")
            .add_url_button("Buy Now", "https://example.com/item2");

        let msg = InteractiveMessageBuilder::new("Check out our catalog!")
            .footer("Swipe right for more items")
            .add_carousel_card(card1)
            .add_carousel_card(card2)
            .build();

        assert!(msg.view_once_message.is_some());
        let inner = msg.view_once_message.unwrap().message.unwrap();
        assert_eq!(
            inner
                .message_context_info
                .as_ref()
                .and_then(|c| c.device_list_metadata_version),
            Some(2)
        );
        let interactive = inner.interactive_message.unwrap();

        match interactive.interactive_message.unwrap() {
            baileys_proto::proto::message::interactive_message::InteractiveMessage::CarouselMessage(carousel) => {
                assert_eq!(carousel.cards.len(), 2);
                assert_eq!(carousel.cards[0].body.as_ref().unwrap().text.as_deref(), Some("Product 1: Rose High-Performance Engine"));
                assert_eq!(carousel.cards[1].body.as_ref().unwrap().text.as_deref(), Some("Product 2: Rose Enterprise Cluster"));
            }
            _ => panic!("Expected CarouselMessage"),
        }
    }
}
