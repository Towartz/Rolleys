//! WhatsApp Business E-commerce, Catalog, Products & Orders Protocol.
//!
//! Provides models, IQ stanza builders, and parsers for:
//! - Merchant product catalogs (`w:biz:catalog`) with cursor pagination
//! - Single product queries and product collections/categories
//! - `ProductMessage` and `OrderMessage` protobuf card generation
//! - Local caching converters to/from `baileys-store` models.

use baileys_binary::BinaryNode;
use baileys_proto::proto::message::{order_message, product_message, OrderMessage, ProductMessage};
use baileys_proto::proto::Message;
use serde::{Deserialize, Serialize};

use crate::CoreError;

/// Details of a WhatsApp Business catalog product.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogProduct {
    pub id: String,
    pub biz_jid: String,
    pub title: String,
    pub description: Option<String>,
    pub currency: String,
    pub price_amount_1000: i64,
    pub sale_price_amount_1000: Option<i64>,
    pub retailer_id: Option<String>,
    pub url: Option<String>,
    pub image_urls: Vec<String>,
    pub is_hidden: bool,
    pub availability: String, // "in_stock" | "out_of_stock"
}

/// A collection or category of products from a business catalog.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogCollection {
    pub id: String,
    pub biz_jid: String,
    pub name: String,
    pub products_count: u32,
    pub status: String,
}

/// Result returned from a catalog query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogResult {
    pub products: Vec<CatalogProduct>,
    pub cursor: Option<String>,
}

/// Result returned from a collections query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollectionsResult {
    pub collections: Vec<CatalogCollection>,
    pub cursor: Option<String>,
}

/// Order metadata representation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderInfo {
    pub order_id: String,
    pub chat_jid: String,
    pub seller_jid: String,
    pub item_count: i32,
    pub status: String,
    pub surface: Option<String>,
    pub order_title: Option<String>,
    pub message: Option<String>,
    pub total_amount_1000: i64,
    pub total_currency_code: String,
    pub token: Option<String>,
    pub created_at: u64,
}

impl From<CatalogProduct> for baileys_store::StoredCatalogProduct {
    fn from(p: CatalogProduct) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            product_id: p.id,
            business_owner_jid: p.biz_jid,
            title: p.title,
            description: p.description,
            currency: p.currency,
            price_amount_1000: p.price_amount_1000,
            sale_price_amount_1000: p.sale_price_amount_1000,
            retailer_id: p.retailer_id,
            url: p.url,
            image_urls: p.image_urls,
            is_hidden: p.is_hidden,
            availability: p.availability,
            updated_at: now,
        }
    }
}

impl From<baileys_store::StoredCatalogProduct> for CatalogProduct {
    fn from(p: baileys_store::StoredCatalogProduct) -> Self {
        Self {
            id: p.product_id,
            biz_jid: p.business_owner_jid,
            title: p.title,
            description: p.description,
            currency: p.currency,
            price_amount_1000: p.price_amount_1000,
            sale_price_amount_1000: p.sale_price_amount_1000,
            retailer_id: p.retailer_id,
            url: p.url,
            image_urls: p.image_urls,
            is_hidden: p.is_hidden,
            availability: p.availability,
        }
    }
}

impl From<CatalogCollection> for baileys_store::StoredCatalogCollection {
    fn from(c: CatalogCollection) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            collection_id: c.id,
            business_owner_jid: c.biz_jid,
            name: c.name,
            products_count: c.products_count,
            status: c.status,
            updated_at: now,
        }
    }
}

impl From<baileys_store::StoredCatalogCollection> for CatalogCollection {
    fn from(c: baileys_store::StoredCatalogCollection) -> Self {
        Self {
            id: c.collection_id,
            biz_jid: c.business_owner_jid,
            name: c.name,
            products_count: c.products_count,
            status: c.status,
        }
    }
}

impl From<OrderInfo> for baileys_store::StoredOrder {
    fn from(o: OrderInfo) -> Self {
        Self {
            order_id: o.order_id,
            chat_jid: o.chat_jid,
            seller_jid: o.seller_jid,
            item_count: o.item_count,
            status: o.status,
            surface: o.surface,
            order_title: o.order_title,
            message: o.message,
            total_amount_1000: o.total_amount_1000,
            total_currency_code: o.total_currency_code,
            token: o.token,
            created_at: o.created_at,
        }
    }
}

impl From<baileys_store::StoredOrder> for OrderInfo {
    fn from(o: baileys_store::StoredOrder) -> Self {
        Self {
            order_id: o.order_id,
            chat_jid: o.chat_jid,
            seller_jid: o.seller_jid,
            item_count: o.item_count,
            status: o.status,
            surface: o.surface,
            order_title: o.order_title,
            message: o.message,
            total_amount_1000: o.total_amount_1000,
            total_currency_code: o.total_currency_code,
            token: o.token,
            created_at: o.created_at,
        }
    }
}

/// Helper to safely retrieve node content as text.
fn get_child_text(node: &BinaryNode, tag: &str) -> Option<String> {
    node.get_child(tag).and_then(|c| {
        c.content_as_bytes()
            .map(|b| String::from_utf8_lossy(b).trim().to_string())
    })
}

/// Builds an IQ node to fetch a business catalog.
pub fn build_get_catalog_iq(biz_jid: &str, limit: u32, cursor: Option<&str>) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let mut req = BinaryNode::new("request")
        .with_attr("type", "catalog")
        .with_attr("biz_jid", biz_jid)
        .with_child(BinaryNode::new("limit").with_string_content(limit.to_string()))
        .with_child(BinaryNode::new("width").with_string_content("100"))
        .with_child(BinaryNode::new("height").with_string_content("100"));

    if let Some(c) = cursor {
        req = req.with_child(BinaryNode::new("after").with_string_content(c));
    }

    BinaryNode::new("iq")
        .with_attr("to", "s.whatsapp.net")
        .with_attr("type", "get")
        .with_attr("xmlns", "w:biz:catalog")
        .with_attr("id", &iq_id)
        .with_child(req)
}

/// Builds an IQ node to fetch a single business product by ID.
pub fn build_get_product_iq(biz_jid: &str, product_id: &str) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let req = BinaryNode::new("request")
        .with_attr("type", "product")
        .with_attr("biz_jid", biz_jid)
        .with_child(BinaryNode::new("product_id").with_string_content(product_id))
        .with_child(BinaryNode::new("width").with_string_content("100"))
        .with_child(BinaryNode::new("height").with_string_content("100"));

    BinaryNode::new("iq")
        .with_attr("to", "s.whatsapp.net")
        .with_attr("type", "get")
        .with_attr("xmlns", "w:biz:catalog")
        .with_attr("id", &iq_id)
        .with_child(req)
}

/// Builds an IQ node to fetch collections / categories from a business catalog.
pub fn build_get_collections_iq(biz_jid: &str, limit: u32, cursor: Option<&str>) -> BinaryNode {
    let iq_id = format!("3EB0{:X}", rand::random::<u64>());
    let mut req = BinaryNode::new("request")
        .with_attr("type", "collections")
        .with_attr("biz_jid", biz_jid)
        .with_child(BinaryNode::new("limit").with_string_content(limit.to_string()))
        .with_child(BinaryNode::new("width").with_string_content("100"))
        .with_child(BinaryNode::new("height").with_string_content("100"));

    if let Some(c) = cursor {
        req = req.with_child(BinaryNode::new("after").with_string_content(c));
    }

    BinaryNode::new("iq")
        .with_attr("to", "s.whatsapp.net")
        .with_attr("type", "get")
        .with_attr("xmlns", "w:biz:catalog")
        .with_attr("id", &iq_id)
        .with_child(req)
}

/// Parses a single `<product>` BinaryNode.
pub fn parse_product_node(node: &BinaryNode, biz_jid: &str) -> Result<CatalogProduct, CoreError> {
    let id = get_child_text(node, "id")
        .or_else(|| node.get_attr("id").map(String::from))
        .ok_or_else(|| CoreError::Protocol("Missing product id in product stanza".to_string()))?;

    let title = get_child_text(node, "title")
        .or_else(|| node.get_attr("title").map(String::from))
        .unwrap_or_default();

    let description = get_child_text(node, "description");
    let currency = get_child_text(node, "currency")
        .or_else(|| node.get_attr("currency").map(String::from))
        .unwrap_or_else(|| "USD".to_string());

    let price_amount_1000 = get_child_text(node, "price")
        .or_else(|| node.get_attr("price").map(String::from))
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    let sale_price_amount_1000 = get_child_text(node, "sale_price")
        .or_else(|| node.get_attr("sale_price").map(String::from))
        .and_then(|s| s.parse::<i64>().ok());

    let retailer_id = get_child_text(node, "retailer_id");
    let url = get_child_text(node, "url");

    let is_hidden = get_child_text(node, "is_hidden")
        .map(|s| s == "true" || s == "1")
        .unwrap_or(false);

    let availability =
        get_child_text(node, "availability").unwrap_or_else(|| "in_stock".to_string());

    let mut image_urls = Vec::new();
    for child in node.get_children() {
        if child.tag == "image" {
            if let Some(u) =
                get_child_text(child, "url").or_else(|| child.get_attr("url").map(String::from))
            {
                image_urls.push(u);
            }
        }
    }

    Ok(CatalogProduct {
        id,
        biz_jid: biz_jid.to_string(),
        title,
        description,
        currency,
        price_amount_1000,
        sale_price_amount_1000,
        retailer_id,
        url,
        image_urls,
        is_hidden,
        availability,
    })
}

/// Parses a catalog response containing a list of `<product>` nodes and optional cursor.
pub fn parse_catalog_response(
    node: &BinaryNode,
    biz_jid: &str,
) -> Result<CatalogResult, CoreError> {
    let resp = if node.tag == "response" {
        node
    } else if let Some(r) = node.get_child("response") {
        r
    } else {
        node
    };

    let mut products = Vec::new();
    for child in resp.get_children() {
        if child.tag == "product" {
            if let Ok(p) = parse_product_node(child, biz_jid) {
                products.push(p);
            }
        }
    }

    let cursor = resp
        .get_child("paging")
        .and_then(|p| get_child_text(p, "cursor"))
        .or_else(|| get_child_text(resp, "cursor"));

    Ok(CatalogResult { products, cursor })
}

/// Parses a collections response containing `<collection>` nodes and optional cursor.
pub fn parse_collections_response(
    node: &BinaryNode,
    biz_jid: &str,
) -> Result<CollectionsResult, CoreError> {
    let resp = if node.tag == "response" {
        node
    } else if let Some(r) = node.get_child("response") {
        r
    } else {
        node
    };

    let mut collections = Vec::new();
    for child in resp.get_children() {
        if child.tag == "collection" {
            let id = get_child_text(child, "id")
                .or_else(|| child.get_attr("id").map(String::from))
                .unwrap_or_default();
            let name = get_child_text(child, "name")
                .or_else(|| child.get_attr("name").map(String::from))
                .unwrap_or_default();
            let products_count = get_child_text(child, "products_count")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            let status = get_child_text(child, "status").unwrap_or_else(|| "active".to_string());

            collections.push(CatalogCollection {
                id,
                biz_jid: biz_jid.to_string(),
                name,
                products_count,
                status,
            });
        }
    }

    let cursor = resp
        .get_child("paging")
        .and_then(|p| get_child_text(p, "cursor"))
        .or_else(|| get_child_text(resp, "cursor"));

    Ok(CollectionsResult {
        collections,
        cursor,
    })
}

/// Builds a `ProductMessage` protobuf card.
pub fn build_product_message(
    biz_owner_jid: &str,
    product: &CatalogProduct,
    caption: Option<&str>,
    footer: Option<&str>,
) -> Message {
    let snapshot = product_message::ProductSnapshot {
        product_id: Some(product.id.clone()),
        title: Some(product.title.clone()),
        description: product.description.clone(),
        currency_code: Some(product.currency.clone()),
        price_amount1000: Some(product.price_amount_1000),
        sale_price_amount1000: product.sale_price_amount_1000,
        retailer_id: product.retailer_id.clone(),
        url: product.url.clone(),
        product_image: None,
        product_image_count: Some(product.image_urls.len() as u32),
        first_image_id: product.image_urls.first().cloned(),
        signed_url: None,
    };

    let prod_msg = ProductMessage {
        product: Some(Box::new(snapshot)),
        business_owner_jid: Some(biz_owner_jid.to_string()),
        catalog: None,
        body: caption.map(|s| s.to_string()),
        footer: footer.map(|s| s.to_string()),
        context_info: None,
    };

    Message {
        product_message: Some(Box::new(prod_msg)),
        ..Default::default()
    }
}

/// Builds an `OrderMessage` protobuf card.
pub fn build_order_message(
    order_id: &str,
    seller_jid: &str,
    title: &str,
    item_count: i32,
    total_amount_1000: i64,
    currency: &str,
    message: Option<&str>,
    thumbnail: Option<Vec<u8>>,
) -> Message {
    let ord = OrderMessage {
        order_id: Some(order_id.to_string()),
        thumbnail,
        item_count: Some(item_count),
        status: Some(order_message::OrderStatus::Inquiry as i32),
        surface: Some(order_message::OrderSurface::Catalog as i32),
        message: message.map(|s| s.to_string()),
        order_title: Some(title.to_string()),
        seller_jid: Some(seller_jid.to_string()),
        token: None,
        total_amount1000: Some(total_amount_1000),
        total_currency_code: Some(currency.to_string()),
        context_info: None,
        message_version: Some(1),
        order_request_message_id: None,
        catalog_type: Some("CATALOG".to_string()),
    };

    Message {
        order_message: Some(Box::new(ord)),
        ..Default::default()
    }
}

/// Converts a `CatalogProduct` into a `<product>` BinaryNode for catalog mutations.
pub fn to_product_node(product_id: Option<&str>, product: &CatalogProduct) -> BinaryNode {
    let mut children = Vec::new();
    if let Some(id) = product_id {
        children.push(BinaryNode::new("id").with_string_content(id));
    }
    children.push(BinaryNode::new("name").with_string_content(&product.title));
    if let Some(ref desc) = product.description {
        children.push(BinaryNode::new("description").with_string_content(desc));
    }
    for img in &product.image_urls {
        children.push(BinaryNode::new("media").with_child(
            BinaryNode::new("image").with_child(BinaryNode::new("url").with_string_content(img)),
        ));
    }
    children
        .push(BinaryNode::new("price").with_string_content(product.price_amount_1000.to_string()));
    children.push(BinaryNode::new("currency").with_string_content(&product.currency));
    if let Some(ref r_id) = product.retailer_id {
        children.push(BinaryNode::new("retailer_id").with_string_content(r_id));
    }
    if let Some(ref u) = product.url {
        children.push(BinaryNode::new("url").with_string_content(u));
    }

    let mut node = BinaryNode::new("product").with_children(children);
    if product.is_hidden {
        node = node.with_attr("is_hidden", "true");
    }
    node
}

/// Builds an IQ node to add a product to the merchant catalog.
pub fn build_product_create_iq(product: &CatalogProduct) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    let prod_node = to_product_node(None, product);
    let add_node = BinaryNode::new("product_catalog_add")
        .with_attr("v", "1")
        .with_child(prod_node)
        .with_child(BinaryNode::new("width").with_string_content("100"))
        .with_child(BinaryNode::new("height").with_string_content("100"));

    BinaryNode::new("iq")
        .with_attr("to", "s.whatsapp.net")
        .with_attr("type", "set")
        .with_attr("xmlns", "w:biz:catalog")
        .with_attr("id", msg_id)
        .with_child(add_node)
}

/// Builds an IQ node to update an existing product in the merchant catalog.
pub fn build_product_update_iq(product_id: &str, product: &CatalogProduct) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    let prod_node = to_product_node(Some(product_id), product);
    let edit_node = BinaryNode::new("product_catalog_edit")
        .with_attr("v", "1")
        .with_child(prod_node)
        .with_child(BinaryNode::new("width").with_string_content("100"))
        .with_child(BinaryNode::new("height").with_string_content("100"));

    BinaryNode::new("iq")
        .with_attr("to", "s.whatsapp.net")
        .with_attr("type", "set")
        .with_attr("xmlns", "w:biz:catalog")
        .with_attr("id", msg_id)
        .with_child(edit_node)
}

/// Builds an IQ node to delete products from the merchant catalog.
pub fn build_product_delete_iq(product_ids: &[&str]) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    let items: Vec<BinaryNode> = product_ids
        .iter()
        .map(|&id| {
            BinaryNode::new("product").with_child(BinaryNode::new("id").with_string_content(id))
        })
        .collect();

    let del_node = BinaryNode::new("product_catalog_delete")
        .with_attr("v", "1")
        .with_children(items);

    BinaryNode::new("iq")
        .with_attr("to", "s.whatsapp.net")
        .with_attr("type", "set")
        .with_attr("xmlns", "w:biz:catalog")
        .with_attr("id", msg_id)
        .with_child(del_node)
}

/// Builds an IQ node to update the WhatsApp Business profile cover photo.
pub fn build_update_cover_photo_iq(photo_id: &str, token: &str, ts: u64) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    let cover = BinaryNode::new("cover_photo")
        .with_attr("id", photo_id)
        .with_attr("op", "update")
        .with_attr("token", token)
        .with_attr("ts", ts.to_string());

    let profile = BinaryNode::new("business_profile")
        .with_attr("v", "3")
        .with_attr("mutation_type", "delta")
        .with_child(cover);

    BinaryNode::new("iq")
        .with_attr("to", "s.whatsapp.net")
        .with_attr("type", "set")
        .with_attr("xmlns", "w:biz")
        .with_attr("id", msg_id)
        .with_child(profile)
}

/// Builds an IQ node to remove the WhatsApp Business profile cover photo.
pub fn build_remove_cover_photo_iq(photo_id: &str) -> BinaryNode {
    let msg_id = format!("3EB0{:X}", rand::random::<u64>());
    let cover = BinaryNode::new("cover_photo")
        .with_attr("id", photo_id)
        .with_attr("op", "delete");

    let profile = BinaryNode::new("business_profile")
        .with_attr("v", "3")
        .with_attr("mutation_type", "delta")
        .with_child(cover);

    BinaryNode::new("iq")
        .with_attr("to", "s.whatsapp.net")
        .with_attr("type", "set")
        .with_attr("xmlns", "w:biz")
        .with_attr("id", msg_id)
        .with_child(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commerce_iq_builders() {
        let cat_iq = build_get_catalog_iq("62812345678@s.whatsapp.net", 10, Some("cursor_123"));
        assert_eq!(cat_iq.tag, "iq");
        assert_eq!(cat_iq.get_attr("xmlns"), Some("w:biz:catalog"));
        let req = cat_iq.get_child("request").expect("Missing request");
        assert_eq!(req.get_attr("type"), Some("catalog"));
        assert_eq!(
            req.get_child("limit").and_then(|c| c.content_as_string()),
            Some("10")
        );
        assert_eq!(
            req.get_child("after").and_then(|c| c.content_as_string()),
            Some("cursor_123")
        );

        let prod_iq = build_get_product_iq("62812345678@s.whatsapp.net", "prod_99");
        let preq = prod_iq.get_child("request").expect("Missing request");
        assert_eq!(preq.get_attr("type"), Some("product"));
        assert_eq!(
            preq.get_child("product_id")
                .and_then(|c| c.content_as_string()),
            Some("prod_99")
        );

        let col_iq = build_get_collections_iq("62812345678@s.whatsapp.net", 5, None);
        let creq = col_iq.get_child("request").expect("Missing request");
        assert_eq!(creq.get_attr("type"), Some("collections"));
        assert!(creq.get_child("after").is_none());
    }

    #[test]
    fn test_parse_catalog_and_product_response() {
        let xml_product = BinaryNode::new("product")
            .with_child(BinaryNode::new("id").with_string_content("p100"))
            .with_child(BinaryNode::new("title").with_string_content("Rose Tea"))
            .with_child(
                BinaryNode::new("description").with_string_content("Delicious rose petal tea"),
            )
            .with_child(BinaryNode::new("price").with_string_content("15000"))
            .with_child(BinaryNode::new("currency").with_string_content("IDR"))
            .with_child(BinaryNode::new("retailer_id").with_string_content("TEA-01"))
            .with_child(BinaryNode::new("availability").with_string_content("in_stock"))
            .with_child(BinaryNode::new("image").with_child(
                BinaryNode::new("url").with_string_content("https://cdn.example.com/tea.jpg"),
            ));

        let prod = parse_product_node(&xml_product, "62812345678@s.whatsapp.net")
            .expect("Failed to parse product");
        assert_eq!(prod.id, "p100");
        assert_eq!(prod.title, "Rose Tea");
        assert_eq!(prod.price_amount_1000, 15000);
        assert_eq!(prod.currency, "IDR");
        assert_eq!(prod.image_urls.len(), 1);
        assert_eq!(prod.image_urls[0], "https://cdn.example.com/tea.jpg");

        let xml_catalog =
            BinaryNode::new("iq").with_child(
                BinaryNode::new("response")
                    .with_attr("type", "catalog")
                    .with_child(xml_product)
                    .with_child(BinaryNode::new("paging").with_child(
                        BinaryNode::new("cursor").with_string_content("next_page_token"),
                    )),
            );

        let res = parse_catalog_response(&xml_catalog, "62812345678@s.whatsapp.net")
            .expect("Failed to parse catalog");
        assert_eq!(res.products.len(), 1);
        assert_eq!(res.products[0].id, "p100");
        assert_eq!(res.cursor.as_deref(), Some("next_page_token"));
    }

    #[test]
    fn test_parse_collections_response() {
        let xml_col = BinaryNode::new("iq").with_child(
            BinaryNode::new("response")
                .with_attr("type", "collections")
                .with_child(
                    BinaryNode::new("collection")
                        .with_child(BinaryNode::new("id").with_string_content("col_spring"))
                        .with_child(BinaryNode::new("name").with_string_content("Spring Specials"))
                        .with_child(BinaryNode::new("products_count").with_string_content("8"))
                        .with_child(BinaryNode::new("status").with_string_content("active")),
                ),
        );

        let res = parse_collections_response(&xml_col, "62812345678@s.whatsapp.net")
            .expect("Failed to parse collections");
        assert_eq!(res.collections.len(), 1);
        assert_eq!(res.collections[0].id, "col_spring");
        assert_eq!(res.collections[0].name, "Spring Specials");
        assert_eq!(res.collections[0].products_count, 8);
    }

    #[test]
    fn test_build_product_and_order_messages() {
        let prod = CatalogProduct {
            id: "p200".to_string(),
            biz_jid: "merchant@s.whatsapp.net".to_string(),
            title: "Vintage Watch".to_string(),
            description: Some("Classic mechanical watch".to_string()),
            currency: "USD".to_string(),
            price_amount_1000: 250000,
            sale_price_amount_1000: None,
            retailer_id: Some("SKU-WATCH-01".to_string()),
            url: Some("https://shop.example.com/watch".to_string()),
            image_urls: vec!["https://cdn.example.com/watch.png".to_string()],
            is_hidden: false,
            availability: "in_stock".to_string(),
        };

        let msg = build_product_message(
            "merchant@s.whatsapp.net",
            &prod,
            Some("Check this out!"),
            Some("Store Footer"),
        );
        let pmsg = msg.product_message.expect("Missing product_message");
        assert_eq!(
            pmsg.business_owner_jid.as_deref(),
            Some("merchant@s.whatsapp.net")
        );
        assert_eq!(pmsg.body.as_deref(), Some("Check this out!"));
        let snap = pmsg.product.expect("Missing snapshot");
        assert_eq!(snap.product_id.as_deref(), Some("p200"));
        assert_eq!(snap.title.as_deref(), Some("Vintage Watch"));
        assert_eq!(snap.price_amount1000, Some(250000));

        let ord_msg = build_order_message(
            "ord_555",
            "merchant@s.whatsapp.net",
            "Order #555",
            3,
            750000,
            "USD",
            Some("Fast shipping"),
            None,
        );
        let omsg = ord_msg.order_message.expect("Missing order_message");
        assert_eq!(omsg.order_id.as_deref(), Some("ord_555"));
        assert_eq!(omsg.order_title.as_deref(), Some("Order #555"));
        assert_eq!(omsg.total_amount1000, Some(750000));
        assert_eq!(omsg.total_currency_code.as_deref(), Some("USD"));
    }
}
