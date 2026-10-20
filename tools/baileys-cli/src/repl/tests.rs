#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_prompt() {
        let connecting_prompt = get_prompt(false);
        assert_eq!(connecting_prompt, "rose[connecting] > ");
        let highlighted_connecting = RoseHelper.highlight_prompt(&connecting_prompt, true);
        assert!(highlighted_connecting.contains("\x1b[33m")); // Yellow

        let online_prompt = get_prompt(true);
        assert_eq!(online_prompt, "rose[online] > ");
        let highlighted_online = RoseHelper.highlight_prompt(&online_prompt, true);
        assert!(highlighted_online.contains("\x1b[92m")); // Bright green
    }

    #[test]
    fn test_rose_helper_completer() {
        let helper = RoseHelper;
        let dummy_ctx = rustyline::history::DefaultHistory::new();
        let (start, matches) = helper.complete("se", 2, &Context::new(&dummy_ctx)).unwrap();
        assert_eq!(start, 0);
        let matches_strs: Vec<String> = matches.into_iter().map(|m| m.replacement).collect();
        assert!(matches_strs.contains(&"send".to_string()));
        assert!(matches_strs.contains(&"selftest".to_string()));
        assert!(matches_strs.contains(&"sendevent".to_string()));
        assert!(matches_strs.contains(&"sendjson".to_string()));

        let (_, g_matches) = helper.complete("gh", 2, &Context::new(&dummy_ctx)).unwrap();
        let g_strs: Vec<String> = g_matches.into_iter().map(|m| m.replacement).collect();
        assert!(g_strs.contains(&"ghostmode".to_string()));

        let (_, f_matches) = helper.complete("fa", 2, &Context::new(&dummy_ctx)).unwrap();
        let f_strs: Vec<String> = f_matches.into_iter().map(|m| m.replacement).collect();
        assert!(f_strs.contains(&"fakevn".to_string()));
        assert!(f_strs.contains(&"fakequote".to_string()));

        let (start2, matches2) = helper
            .complete("qr l", 4, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start2, 3);
        let matches2_strs: Vec<String> = matches2.into_iter().map(|m| m.replacement).collect();
        assert!(matches2_strs.contains(&"large".to_string()));

        let (start3, matches3) = helper
            .complete("proto s", 7, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start3, 6);
        let matches3_strs: Vec<String> = matches3.into_iter().map(|m| m.replacement).collect();
        assert!(matches3_strs.contains(&"search".to_string()));

        let (start4, matches4) = helper
            .complete("cmdmode p", 9, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start4, 8);
        let matches4_strs: Vec<String> = matches4.into_iter().map(|m| m.replacement).collect();
        assert!(matches4_strs.contains(&"public".to_string()));

        let (start5, matches5) = helper
            .complete("cmdm", 4, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start5, 0);
        let matches5_strs: Vec<String> = matches5.into_iter().map(|m| m.replacement).collect();
        assert!(matches5_strs.contains(&"cmdmode".to_string()));

        let (start6, matches6) = helper
            .complete("logfilter d", 11, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start6, 10);
        let matches6_strs: Vec<String> = matches6.into_iter().map(|m| m.replacement).collect();
        assert!(matches6_strs.contains(&"debug".to_string()));

        let (start7, matches7) = helper
            .complete("logfilter t", 11, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start7, 10);
        let matches7_strs: Vec<String> = matches7.into_iter().map(|m| m.replacement).collect();
        assert!(matches7_strs.contains(&"trace".to_string()));

        let (start8, matches8) = helper
            .complete("help mess", 9, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start8, 5);
        let matches8_strs: Vec<String> = matches8.into_iter().map(|m| m.replacement).collect();
        assert!(matches8_strs.contains(&"messaging".to_string()));

        let (start9, matches9) = helper
            .complete("help poll", 9, &Context::new(&dummy_ctx))
            .unwrap();
        assert_eq!(start9, 5);
        let matches9_strs: Vec<String> = matches9.into_iter().map(|m| m.replacement).collect();
        assert!(matches9_strs.contains(&"poll".to_string()));
        assert!(matches9_strs.contains(&"polltally".to_string()));
    }

    #[test]
    fn test_parse_target_path_extra() {
        // Plain arguments
        let res1 = parse_target_path_extra("me C:\\test\\file.jpg hello caption").unwrap();
        assert_eq!(res1.0, "me");
        assert_eq!(res1.1, "C:\\test\\file.jpg");
        assert_eq!(res1.2, Some("hello caption".to_string()));

        // Quoted path with spaces
        let res2 = parse_target_path_extra(
            "62812345678 \"C:\\Users\\Admin\\Downloads\\my photo.png\" Cute photo!",
        )
        .unwrap();
        assert_eq!(res2.0, "62812345678");
        assert_eq!(res2.1, "C:\\Users\\Admin\\Downloads\\my photo.png");
        assert_eq!(res2.2, Some("Cute photo!".to_string()));

        // Single quotes without caption
        let res3 = parse_target_path_extra("self 'C:\\sticker.webp'").unwrap();
        assert_eq!(res3.0, "self");
        assert_eq!(res3.1, "C:\\sticker.webp");
        assert_eq!(res3.2, None);
    }

    #[test]
    fn test_proto_schema_and_recognizer_integration() {
        let stats = SchemaInspector::global().stats();
        assert!(stats.total_messages > 100);
        assert!(stats.total_enums > 10);
        assert!(stats.total_fields > 500);

        // Search test
        let matches = SchemaInspector::global().search("poll");
        assert!(!matches.is_empty());

        // Definition view test
        let def = SchemaInspector::global().get_definition("ReactionMessage");
        assert!(def.is_some());
        assert!(def.unwrap().contains("text"));

        // Recognizer test
        let msg = baileys_proto::proto::Message {
            conversation: Some("Test message".to_string()),
            ..Default::default()
        };
        let unwrapped = msg.unwrap_wrappers();
        assert_eq!(unwrapped.real_type, RecognizedMessageType::Conversation);
        assert_eq!(msg.extract_text(), Some("Test message".to_string()));
    }
}
