//! Command Registry, Metadata & Help System for Rose-Baileys CLI.
//!
//! Provides two-stage command dispatching, detailed parameter validation,
//! typo suggestions ("Did you mean?"), and categorized contextual help.

use std::fmt;

/// Categories for REPL commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    Messaging,
    Groups,
    Status,
    Channels,
    Community,
    Commerce,
    Privacy,
    WAProto,
    System,
}

impl CommandCategory {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Messaging => "messaging",
            Self::Groups => "groups",
            Self::Status => "status",
            Self::Channels => "channels",
            Self::Community => "community",
            Self::Commerce => "commerce",
            Self::Privacy => "privacy",
            Self::WAProto => "proto",
            Self::System => "system",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Messaging => "Messaging & Interactive Rich Content",
            Self::Groups => "Group Management & Moderation",
            Self::Status => "Status Stories & Ephemeral Media",
            Self::Channels => "WhatsApp Newsletters & Channels",
            Self::Community => "WhatsApp Communities & Subgroups",
            Self::Commerce => "Catalog, Products & Commerce Orders",
            Self::Privacy => "Privacy, Blocklist & Security",
            Self::WAProto => "WAProto Intelligence Suite",
            Self::System => "System, Session, Diagnostics & Tools",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "messaging" | "message" | "msg" => Some(Self::Messaging),
            "groups" | "group" | "grp" => Some(Self::Groups),
            "status" | "story" | "stories" => Some(Self::Status),
            "channels" | "channel" | "newsletter" | "newsletters" => Some(Self::Channels),
            "community" | "comm" => Some(Self::Community),
            "commerce" | "biz" | "shop" | "catalog" | "product" => Some(Self::Commerce),
            "privacy" | "security" | "block" => Some(Self::Privacy),
            "proto" | "waproto" | "schema" => Some(Self::WAProto),
            "system" | "sys" | "session" | "tools" => Some(Self::System),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn all() -> &'static [CommandCategory] {
        &[
            Self::Messaging,
            Self::Groups,
            Self::Status,
            Self::Channels,
            Self::Community,
            Self::Commerce,
            Self::Privacy,
            Self::WAProto,
            Self::System,
        ]
    }
}

impl fmt::Display for CommandCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Metadata specification for a REPL command.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub category: CommandCategory,
    pub summary: &'static str,
    pub usage: &'static str,
    pub example: &'static str,
    #[allow(dead_code)]
    pub min_args: usize,
    pub missing_hint: &'static str,
}

pub static COMMANDS: &[CommandSpec] = &[
    // ─── Messaging ───────────────────────────────────────────────────────────
    CommandSpec {
        name: "send",
        aliases: &["msg", "m"],
        category: CommandCategory::Messaging,
        summary: "Send a text message to a user or group",
        usage: "send <JID> <TEXT>",
        example: "send 62812345678@s.whatsapp.net Hello from Rose-Baileys!",
        min_args: 2,
        missing_hint: "<JID> <TEXT>",
    },
    CommandSpec {
        name: "sendbuttons",
        aliases: &["buttons"],
        category: CommandCategory::Messaging,
        summary: "Send interactive native flow action buttons (quick reply, URL, call)",
        usage: "sendbuttons <JID> <BODY> | <TITLE:URL|REPLY:PARAM>...",
        example: "sendbuttons 62812345678@s.whatsapp.net Hello! | Confirm:btn_1 | Visit:https://wa.me",
        min_args: 2,
        missing_hint: "<JID> <BODY> | <BUTTON_SPEC>...",
    },
    CommandSpec {
        name: "sendlist",
        aliases: &["listmsg"],
        category: CommandCategory::Messaging,
        summary: "Send an interactive list menu with single-selection rows",
        usage: "sendlist <JID> <TITLE> | <BTN_TEXT> | <SEC_NAME> | <ROW_TITLE:ROW_DESC:ROW_ID>...",
        example: "sendlist 62812345678@s.whatsapp.net Menu | Open Menu | Drinks | Coffee:Espresso:c1 | Tea:Green:t1",
        min_args: 4,
        missing_hint: "<JID> <TITLE> | <BTN_TEXT> | <SEC_NAME> | <ROW_SPECS...>",
    },
    CommandSpec {
        name: "sendquickreply",
        aliases: &["quickreply"],
        category: CommandCategory::Messaging,
        summary: "Send native flow quick reply buttons",
        usage: "sendquickreply <JID> <TEXT> | <BTN1_ID:BTN1_TEXT>...",
        example: "sendquickreply 62812345678@s.whatsapp.net Choose option: | opt_yes:Yes | opt_no:No",
        min_args: 2,
        missing_hint: "<JID> <TEXT> | <BTN_ID:BTN_TEXT>...",
    },
    CommandSpec {
        name: "sendcarousel",
        aliases: &["carousel"],
        category: CommandCategory::Messaging,
        summary: "Send a swipeable multi-card carousel message",
        usage: "sendcarousel <JID> <TITLE> | <CARD1_TITLE:BTN_TEXT:URL>...",
        example: "sendcarousel 62812345678@s.whatsapp.net Catalog | Product A:Buy:https://wa.me/p1 | Product B:Buy:https://wa.me/p2",
        min_args: 2,
        missing_hint: "<JID> <TITLE> | <CARD_SPECS...>",
    },
    CommandSpec {
        name: "poll",
        aliases: &[],
        category: CommandCategory::Messaging,
        summary: "Create an interactive WhatsApp poll with voter options",
        usage: "poll <JID> <QUESTION> | <OPT1,OPT2...> [| SELECTABLE_COUNT]",
        example: "poll 62812345678@s.whatsapp.net Lunch menu? | Pizza,Burger,Nasi Goreng | 1",
        min_args: 2,
        missing_hint: "<JID> <QUESTION> | <OPTIONS>",
    },
    CommandSpec {
        name: "vote",
        aliases: &[],
        category: CommandCategory::Messaging,
        summary: "Cast encrypted vote on an existing poll (AES-256-GCM)",
        usage: "vote <JID> <POLL_ID> <OPTION1,OPTION2...>",
        example: "vote 62812345678@s.whatsapp.net 3EB0A1B2C3 Pizza",
        min_args: 3,
        missing_hint: "<JID> <POLL_ID> <OPTIONS>",
    },
    CommandSpec {
        name: "polltally",
        aliases: &["tally"],
        category: CommandCategory::Messaging,
        summary: "View vote results and option tally for a poll",
        usage: "polltally <POLL_ID>",
        example: "polltally 3EB0A1B2C3",
        min_args: 1,
        missing_hint: "<POLL_ID>",
    },
    CommandSpec {
        name: "reply",
        aliases: &["quote"],
        category: CommandCategory::Messaging,
        summary: "Send a contextual reply quoting a previous message",
        usage: "reply <JID> <MSG_ID> <TEXT>",
        example: "reply 62812345678@s.whatsapp.net 3EB01234 Got your message!",
        min_args: 3,
        missing_hint: "<JID> <MSG_ID> <TEXT>",
    },
    CommandSpec {
        name: "react",
        aliases: &["reaction"],
        category: CommandCategory::Messaging,
        summary: "Send an emoji reaction to a specific message",
        usage: "react <JID> <MSG_ID> <EMOJI>",
        example: "react 62812345678@s.whatsapp.net 3EB01234 ❤️",
        min_args: 3,
        missing_hint: "<JID> <MSG_ID> <EMOJI>",
    },
    CommandSpec {
        name: "delete",
        aliases: &["revoke"],
        category: CommandCategory::Messaging,
        summary: "Delete a sent message for everyone",
        usage: "delete <JID> <MSG_ID>",
        example: "delete 62812345678@s.whatsapp.net 3EB01234",
        min_args: 2,
        missing_hint: "<JID> <MSG_ID>",
    },
    CommandSpec {
        name: "edit",
        aliases: &[],
        category: CommandCategory::Messaging,
        summary: "Edit previously sent message text",
        usage: "edit <JID> <MSG_ID> <NEW_TEXT>",
        example: "edit 62812345678@s.whatsapp.net 3EB01234 Corrected text here",
        min_args: 3,
        missing_hint: "<JID> <MSG_ID> <NEW_TEXT>",
    },
    CommandSpec {
        name: "forward",
        aliases: &[],
        category: CommandCategory::Messaging,
        summary: "Forward an existing message from store to another chat",
        usage: "forward <TO_JID> <FROM_JID> <MSG_ID>",
        example: "forward 6289999999@s.whatsapp.net 62812345678@s.whatsapp.net 3EB01234",
        min_args: 3,
        missing_hint: "<TO_JID> <FROM_JID> <MSG_ID>",
    },
    CommandSpec {
        name: "downloadmedia",
        aliases: &["download", "dl", "dlmedia"],
        category: CommandCategory::Messaging,
        summary: "Stream-download & decrypt media attachment with progress (~64KB RAM)",
        usage: "downloadmedia <MSG_ID> [DEST_PATH]",
        example: "downloadmedia 3EB01234 ./downloads/photo.jpg",
        min_args: 1,
        missing_hint: "<MSG_ID>",
    },
    CommandSpec {
        name: "sendmedia",
        aliases: &["streamupload", "uploadmedia"],
        category: CommandCategory::Messaging,
        summary: "Stream-upload and send large media file with real-time progress (~64KB RAM)",
        usage: "sendmedia <JID> <PATH> <TYPE> [CAPTION]",
        example: "sendmedia 62812345678@s.whatsapp.net ./video.mp4 video \"Watch this!\"",
        min_args: 3,
        missing_hint: "<JID> <PATH> <TYPE> [CAPTION]",
    },
    CommandSpec {
        name: "autodownload",
        aliases: &[],
        category: CommandCategory::Messaging,
        summary: "Configure automatic background media download filter",
        usage: "autodownload [all|image|video|audio|document|off]",
        example: "autodownload image",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "sendlocation",
        aliases: &["location"],
        category: CommandCategory::Messaging,
        summary: "Send a static geographic location pin",
        usage: "sendlocation <JID> <LAT> <LNG> [| NAME | ADDRESS]",
        example: "sendlocation 62812345678@s.whatsapp.net -6.2088 106.8456 | Monas | Jakarta Pusat",
        min_args: 3,
        missing_hint: "<JID> <LAT> <LNG>",
    },
    CommandSpec {
        name: "sendlivelocation",
        aliases: &["livelocation"],
        category: CommandCategory::Messaging,
        summary: "Send real-time live location",
        usage: "sendlivelocation <JID> <LAT> <LNG> [| CAPTION]",
        example: "sendlivelocation 62812345678@s.whatsapp.net -6.2088 106.8456 | Sharing for 1 hour",
        min_args: 3,
        missing_hint: "<JID> <LAT> <LNG>",
    },
    CommandSpec {
        name: "sendcontact",
        aliases: &["contact"],
        category: CommandCategory::Messaging,
        summary: "Send contact card (vCard 3.0)",
        usage: "sendcontact <JID> <NAME> <PHONE_NUMBER>",
        example: "sendcontact 62812345678@s.whatsapp.net John Doe +62812345678",
        min_args: 3,
        missing_hint: "<JID> <NAME> <PHONE_NUMBER>",
    },
    CommandSpec {
        name: "sendgroupinvite",
        aliases: &["groupinvitecard"],
        category: CommandCategory::Messaging,
        summary: "Send group invite invitation card",
        usage: "sendgroupinvite <JID> <GROUP_JID> <CODE> <EXPIRATION> <NAME>",
        example: "sendgroupinvite 62812345678@s.whatsapp.net 12345@g.us ABCDEF 1710000000 My Group",
        min_args: 5,
        missing_hint: "<JID> <GROUP_JID> <CODE> <EXPIRATION> <NAME>",
    },
    CommandSpec {
        name: "sendevent",
        aliases: &["event"],
        category: CommandCategory::Messaging,
        summary: "Send interactive calendar event card",
        usage: "sendevent <JID> <TITLE> [| DESC | LOCATION]",
        example: "sendevent 62812345678@s.whatsapp.net Team Meeting | Review Sprint | Zoom",
        min_args: 2,
        missing_hint: "<JID> <TITLE>",
    },
    CommandSpec {
        name: "sendjson",
        aliases: &["jsonmsg"],
        category: CommandCategory::Messaging,
        summary: "Send dynamic/shorthand JSON rich message",
        usage: "sendjson <JID> <RAW_JSON>",
        example: "sendjson 62812345678@s.whatsapp.net {\"text\":\"Hello JSON!\"}",
        min_args: 2,
        missing_hint: "<JID> <RAW_JSON>",
    },
    CommandSpec {
        name: "fakevn",
        aliases: &[],
        category: CommandCategory::Messaging,
        summary: "Send audio file disguised as native Voice Note (PTT)",
        usage: "fakevn <JID> <PATH> [SECONDS]",
        example: "fakevn 62812345678@s.whatsapp.net ./audio.ogg 15",
        min_args: 2,
        missing_hint: "<JID> <PATH>",
    },
    CommandSpec {
        name: "fakequote",
        aliases: &[],
        category: CommandCategory::Messaging,
        summary: "Send forged quoted reply quoting another person",
        usage: "fakequote <JID> <REMOTE_JID> | <FAKE_AUTHOR> | <FAKE_TEXT> | <REAL_REPLY>",
        example: "fakequote 62812345678@s.whatsapp.net 62812345678@s.whatsapp.net | 628999@s.whatsapp.net | Agreed | Noted!",
        min_args: 4,
        missing_hint: "<JID> <REMOTE_JID> | <FAKE_AUTHOR> | <FAKE_TEXT> | <REAL_REPLY>",
    },
    CommandSpec {
        name: "sendpreview",
        aliases: &["urlpreviewsend"],
        category: CommandCategory::Messaging,
        summary: "Send text message with OpenGraph rich link preview",
        usage: "sendpreview <JID> <TEXT>",
        example: "sendpreview 62812345678@s.whatsapp.net Check this: https://github.com/Rose-Project",
        min_args: 2,
        missing_hint: "<JID> <TEXT>",
    },

    // ─── Groups ───────────────────────────────────────────────────────────────
    CommandSpec {
        name: "groupinfo",
        aliases: &["ginfo"],
        category: CommandCategory::Groups,
        summary: "Inspect group metadata, participant list, and permissions",
        usage: "groupinfo <GROUP_JID>",
        example: "groupinfo 120363028123456789@g.us",
        min_args: 1,
        missing_hint: "<GROUP_JID>",
    },
    CommandSpec {
        name: "groupcreate",
        aliases: &["gcreate"],
        category: CommandCategory::Groups,
        summary: "Create a new WhatsApp group",
        usage: "groupcreate <SUBJECT> <PARTICIPANT_JIDS...>",
        example: "groupcreate \"Dev Team\" 62812345678@s.whatsapp.net,62898765432@s.whatsapp.net",
        min_args: 2,
        missing_hint: "<SUBJECT> <PARTICIPANTS...>",
    },
    CommandSpec {
        name: "groupadd",
        aliases: &["gadd"],
        category: CommandCategory::Groups,
        summary: "Add participant(s) to a group",
        usage: "groupadd <GROUP_JID> <PARTICIPANTS...>",
        example: "groupadd 120363028123456789@g.us 62812345678@s.whatsapp.net",
        min_args: 2,
        missing_hint: "<GROUP_JID> <PARTICIPANTS...>",
    },
    CommandSpec {
        name: "groupkick",
        aliases: &["gremove", "gkick"],
        category: CommandCategory::Groups,
        summary: "Remove participant(s) from a group",
        usage: "groupkick <GROUP_JID> <PARTICIPANTS...>",
        example: "groupkick 120363028123456789@g.us 62812345678@s.whatsapp.net",
        min_args: 2,
        missing_hint: "<GROUP_JID> <PARTICIPANTS...>",
    },
    CommandSpec {
        name: "grouppromote",
        aliases: &["gpromote"],
        category: CommandCategory::Groups,
        summary: "Promote participant(s) to group admin",
        usage: "grouppromote <GROUP_JID> <PARTICIPANTS...>",
        example: "grouppromote 120363028123456789@g.us 62812345678@s.whatsapp.net",
        min_args: 2,
        missing_hint: "<GROUP_JID> <PARTICIPANTS...>",
    },
    CommandSpec {
        name: "groupdemote",
        aliases: &["gdemote"],
        category: CommandCategory::Groups,
        summary: "Demote group admin(s) back to regular members",
        usage: "groupdemote <GROUP_JID> <PARTICIPANTS...>",
        example: "groupdemote 120363028123456789@g.us 62812345678@s.whatsapp.net",
        min_args: 2,
        missing_hint: "<GROUP_JID> <PARTICIPANTS...>",
    },
    CommandSpec {
        name: "groupsubject",
        aliases: &["gsubject"],
        category: CommandCategory::Groups,
        summary: "Update group title / subject",
        usage: "groupsubject <GROUP_JID> <NEW_SUBJECT>",
        example: "groupsubject 120363028123456789@g.us \"Rose Rust Team\"",
        min_args: 2,
        missing_hint: "<GROUP_JID> <NEW_SUBJECT>",
    },
    CommandSpec {
        name: "groupdesc",
        aliases: &["gdesc"],
        category: CommandCategory::Groups,
        summary: "Update group description",
        usage: "groupdesc <GROUP_JID> <NEW_DESCRIPTION>",
        example: "groupdesc 120363028123456789@g.us \"Official Rose Project Discussion\"",
        min_args: 2,
        missing_hint: "<GROUP_JID> <NEW_DESCRIPTION>",
    },
    CommandSpec {
        name: "groupsetting",
        aliases: &["gsetting"],
        category: CommandCategory::Groups,
        summary: "Update group permissions (announcement, locked, unlocked)",
        usage: "groupsetting <GROUP_JID> <announcement|not_announcement|locked|unlocked>",
        example: "groupsetting 120363028123456789@g.us announcement",
        min_args: 2,
        missing_hint: "<GROUP_JID> <SETTING>",
    },
    CommandSpec {
        name: "groupinvite",
        aliases: &["ginvite"],
        category: CommandCategory::Groups,
        summary: "Get group invite link code",
        usage: "groupinvite <GROUP_JID>",
        example: "groupinvite 120363028123456789@g.us",
        min_args: 1,
        missing_hint: "<GROUP_JID>",
    },
    CommandSpec {
        name: "groupjoin",
        aliases: &["gjoin"],
        category: CommandCategory::Groups,
        summary: "Join group using invite code",
        usage: "groupjoin <INVITE_CODE>",
        example: "groupjoin ABCdef123456",
        min_args: 1,
        missing_hint: "<INVITE_CODE>",
    },
    CommandSpec {
        name: "groupparticipating",
        aliases: &["glist"],
        category: CommandCategory::Groups,
        summary: "List all participating groups",
        usage: "groupparticipating",
        example: "groupparticipating",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "groupjoinapproval",
        aliases: &[],
        category: CommandCategory::Groups,
        summary: "Toggle group join approval requirement (admin review)",
        usage: "groupjoinapproval <GROUP_JID> <on|off>",
        example: "groupjoinapproval 120363028123456789@g.us on",
        min_args: 2,
        missing_hint: "<GROUP_JID> <on|off>",
    },
    CommandSpec {
        name: "grouprequests",
        aliases: &[],
        category: CommandCategory::Groups,
        summary: "List pending participant join requests for a group",
        usage: "grouprequests <GROUP_JID>",
        example: "grouprequests 120363028123456789@g.us",
        min_args: 1,
        missing_hint: "<GROUP_JID>",
    },
    CommandSpec {
        name: "grouprequestupdate",
        aliases: &[],
        category: CommandCategory::Groups,
        summary: "Approve or reject pending group join requests",
        usage: "grouprequestupdate <GROUP_JID> <approve|reject> <JID1,JID2...>",
        example: "grouprequestupdate 120363028123456789@g.us approve 62812345678@s.whatsapp.net",
        min_args: 3,
        missing_hint: "<GROUP_JID> <approve|reject> <JIDS...>",
    },
    CommandSpec {
        name: "groupmemberaddmode",
        aliases: &[],
        category: CommandCategory::Groups,
        summary: "Configure group member add mode",
        usage: "groupmemberaddmode <GROUP_JID> <admin_add|all_member_add>",
        example: "groupmemberaddmode 120363028123456789@g.us admin_add",
        min_args: 2,
        missing_hint: "<GROUP_JID> <admin_add|all_member_add>",
    },
    CommandSpec {
        name: "grouprevokev4",
        aliases: &[],
        category: CommandCategory::Groups,
        summary: "Revoke v4 group invite for a participant",
        usage: "grouprevokev4 <GROUP_JID> <USER_JID>",
        example: "grouprevokev4 120363028123456789@g.us 62812345678@s.whatsapp.net",
        min_args: 2,
        missing_hint: "<GROUP_JID> <USER_JID>",
    },
    CommandSpec {
        name: "groupacceptv4",
        aliases: &[],
        category: CommandCategory::Groups,
        summary: "Accept v4 group invite",
        usage: "groupacceptv4 <GROUP_JID> <INVITE_CODE> <EXPIRATION> <ADMIN_JID>",
        example: "groupacceptv4 120363028123456789@g.us ABCDEF 1710000000 62812345678@s.whatsapp.net",
        min_args: 4,
        missing_hint: "<GROUP_JID> <INVITE_CODE> <EXPIRATION> <ADMIN_JID>",
    },

    // ─── Status ───────────────────────────────────────────────────────────────
    CommandSpec {
        name: "statuspost",
        aliases: &["poststory"],
        category: CommandCategory::Status,
        summary: "Publish text status story to status@broadcast",
        usage: "statuspost <TEXT> [| BG_HEX]",
        example: "statuspost Hello world from Rose! | #128C7E",
        min_args: 1,
        missing_hint: "<TEXT>",
    },
    CommandSpec {
        name: "statuslink",
        aliases: &[],
        category: CommandCategory::Status,
        summary: "Publish rich link preview story to status@broadcast",
        usage: "statuslink <URL> [| TITLE | DESC | PREVIEW_URL]",
        example: "statuslink https://rust-lang.org | Rust Programming | Blazingly Fast",
        min_args: 1,
        missing_hint: "<URL>",
    },
    CommandSpec {
        name: "statusmedia",
        aliases: &[],
        category: CommandCategory::Status,
        summary: "Publish HD image or video story to status@broadcast",
        usage: "statusmedia <FILE_PATH> [| CAPTION]",
        example: "statusmedia ./photos/nature.jpg | Beautiful morning",
        min_args: 1,
        missing_hint: "<FILE_PATH>",
    },
    CommandSpec {
        name: "statusaudio",
        aliases: &[],
        category: CommandCategory::Status,
        summary: "Publish voice/audio status story to status@broadcast",
        usage: "statusaudio <AUDIO_PATH> [| SECONDS]",
        example: "statusaudio ./recordings/voice.m4a | 30",
        min_args: 1,
        missing_hint: "<AUDIO_PATH>",
    },
    CommandSpec {
        name: "statusview",
        aliases: &["viewstory"],
        category: CommandCategory::Status,
        summary: "Send story read receipt to author (marks as viewed)",
        usage: "statusview <MSG_ID> <AUTHOR_JID>",
        example: "statusview 3EB0A1B2 62812345678@s.whatsapp.net",
        min_args: 2,
        missing_hint: "<MSG_ID> <AUTHOR_JID>",
    },
    CommandSpec {
        name: "stories",
        aliases: &[],
        category: CommandCategory::Status,
        summary: "List recent status stories received from contacts",
        usage: "stories [LIMIT] [unviewed]",
        example: "stories 20 unviewed",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "storiesauthor",
        aliases: &[],
        category: CommandCategory::Status,
        summary: "List status stories by specific author JID",
        usage: "storiesauthor <AUTHOR_JID>",
        example: "storiesauthor 62812345678@s.whatsapp.net",
        min_args: 1,
        missing_hint: "<AUTHOR_JID>",
    },
    CommandSpec {
        name: "statusupdate",
        aliases: &["setabout"],
        category: CommandCategory::Status,
        summary: "Update self WhatsApp profile text status (About)",
        usage: "statusupdate <STATUS_TEXT>",
        example: "statusupdate \"Available on Rose-Baileys\"",
        min_args: 1,
        missing_hint: "<STATUS_TEXT>",
    },
    CommandSpec {
        name: "statusfetch",
        aliases: &["getabout"],
        category: CommandCategory::Status,
        summary: "Fetch contact About status and update timestamp",
        usage: "statusfetch <JID>",
        example: "statusfetch 62812345678@s.whatsapp.net",
        min_args: 1,
        missing_hint: "<JID>",
    },

    // ─── Channels (Newsletters) ───────────────────────────────────────────────
    CommandSpec {
        name: "newsletters",
        aliases: &["channellist"],
        category: CommandCategory::Channels,
        summary: "List subscribed and owned WhatsApp channels",
        usage: "newsletters",
        example: "newsletters",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "newsletter",
        aliases: &["channelinfo"],
        category: CommandCategory::Channels,
        summary: "Inspect metadata and settings for a channel",
        usage: "newsletter <NEWSLETTER_JID>",
        example: "newsletter 120363028123456789@newsletter",
        min_args: 1,
        missing_hint: "<NEWSLETTER_JID>",
    },
    CommandSpec {
        name: "follow",
        aliases: &["channelsub"],
        category: CommandCategory::Channels,
        summary: "Follow / subscribe to a WhatsApp channel",
        usage: "follow <NEWSLETTER_JID>",
        example: "follow 120363028123456789@newsletter",
        min_args: 1,
        missing_hint: "<NEWSLETTER_JID>",
    },
    CommandSpec {
        name: "unfollow",
        aliases: &["channelunsub"],
        category: CommandCategory::Channels,
        summary: "Unfollow / unsubscribe from a WhatsApp channel",
        usage: "unfollow <NEWSLETTER_JID>",
        example: "unfollow 120363028123456789@newsletter",
        min_args: 1,
        missing_hint: "<NEWSLETTER_JID>",
    },
    CommandSpec {
        name: "channelmute",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Mute or unmute notifications for a channel",
        usage: "channelmute <NEWSLETTER_JID> <on|off>",
        example: "channelmute 120363028123456789@newsletter on",
        min_args: 2,
        missing_hint: "<NEWSLETTER_JID> <on|off>",
    },
    CommandSpec {
        name: "channelcreate",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Create a new public WhatsApp channel",
        usage: "channelcreate <NAME> [DESCRIPTION]",
        example: "channelcreate \"Tech News\" \"Daily technology highlights\"",
        min_args: 1,
        missing_hint: "<NAME>",
    },
    CommandSpec {
        name: "channelpost",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Broadcast a text message update to a channel",
        usage: "channelpost <NEWSLETTER_JID> <TEXT>",
        example: "channelpost 120363028123456789@newsletter \"Version 1.0 is released!\"",
        min_args: 2,
        missing_hint: "<NEWSLETTER_JID> <TEXT>",
    },
    CommandSpec {
        name: "channelimage",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Post image media update to a channel",
        usage: "channelimage <NEWSLETTER_JID> <IMAGE_PATH> [| CAPTION]",
        example: "channelimage 120363028123456789@newsletter ./banner.jpg | Release banner",
        min_args: 2,
        missing_hint: "<NEWSLETTER_JID> <IMAGE_PATH>",
    },
    CommandSpec {
        name: "channelstory",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Post text status story into a channel",
        usage: "channelstory <NEWSLETTER_JID> <TEXT> [| BG_HEX]",
        example: "channelstory 120363028123456789@newsletter Hello subscribers! | #00A884",
        min_args: 2,
        missing_hint: "<NEWSLETTER_JID> <TEXT>",
    },
    CommandSpec {
        name: "channelimagestory",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Post image status story into a channel",
        usage: "channelimagestory <NEWSLETTER_JID> <IMAGE_PATH> [| CAPTION]",
        example: "channelimagestory 120363028123456789@newsletter ./photo.jpg | Story caption",
        min_args: 2,
        missing_hint: "<NEWSLETTER_JID> <IMAGE_PATH>",
    },
    CommandSpec {
        name: "channelreact",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Send emoji reaction to a channel update message",
        usage: "channelreact <NEWSLETTER_JID> <SERVER_ID> <EMOJI>",
        example: "channelreact 120363028123456789@newsletter 102 🔥",
        min_args: 3,
        missing_hint: "<NEWSLETTER_JID> <SERVER_ID> <EMOJI>",
    },
    CommandSpec {
        name: "channelmsgs",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Fetch recent channel message history",
        usage: "channelmsgs <NEWSLETTER_JID> [COUNT]",
        example: "channelmsgs 120363028123456789@newsletter 20",
        min_args: 1,
        missing_hint: "<NEWSLETTER_JID>",
    },
    CommandSpec {
        name: "channelpromote",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Promote user to channel administrator",
        usage: "channelpromote <NEWSLETTER_JID> <USER_JID>",
        example: "channelpromote 120363028123456789@newsletter 62812345678@s.whatsapp.net",
        min_args: 2,
        missing_hint: "<NEWSLETTER_JID> <USER_JID>",
    },
    CommandSpec {
        name: "channeldemote",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Demote channel administrator back to subscriber",
        usage: "channeldemote <NEWSLETTER_JID> <USER_JID>",
        example: "channeldemote 120363028123456789@newsletter 62812345678@s.whatsapp.net",
        min_args: 2,
        missing_hint: "<NEWSLETTER_JID> <USER_JID>",
    },
    CommandSpec {
        name: "channeladmincount",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Query administrator count for a channel",
        usage: "channeladmincount <NEWSLETTER_JID>",
        example: "channeladmincount 120363028123456789@newsletter",
        min_args: 1,
        missing_hint: "<NEWSLETTER_JID>",
    },
    CommandSpec {
        name: "channelsublive",
        aliases: &[],
        category: CommandCategory::Channels,
        summary: "Subscribe to live realtime channel updates via IQ",
        usage: "channelsublive <NEWSLETTER_JID>",
        example: "channelsublive 120363028123456789@newsletter",
        min_args: 1,
        missing_hint: "<NEWSLETTER_JID>",
    },

    // ─── Community ────────────────────────────────────────────────────────────
    CommandSpec {
        name: "communitycreate",
        aliases: &["commcreate"],
        category: CommandCategory::Community,
        summary: "Create a new parent WhatsApp Community",
        usage: "communitycreate <NAME> [DESCRIPTION]",
        example: "communitycreate \"Open Source Community\" \"All open source contributors\"",
        min_args: 1,
        missing_hint: "<NAME>",
    },
    CommandSpec {
        name: "communitycreatesub",
        aliases: &["commcreatesub"],
        category: CommandCategory::Community,
        summary: "Create a subgroup linked under parent Community",
        usage: "communitycreatesub <COMMUNITY_JID> <NAME> [PARTICIPANTS...]",
        example: "communitycreatesub 120363028123456789@g.us \"Rust Group\"",
        min_args: 2,
        missing_hint: "<COMMUNITY_JID> <NAME>",
    },
    CommandSpec {
        name: "communitylink",
        aliases: &["commlink"],
        category: CommandCategory::Community,
        summary: "Link existing group into a parent Community",
        usage: "communitylink <COMMUNITY_JID> <GROUP_JID>",
        example: "communitylink 120363028123456789@g.us 120363099999999999@g.us",
        min_args: 2,
        missing_hint: "<COMMUNITY_JID> <GROUP_JID>",
    },
    CommandSpec {
        name: "communityunlink",
        aliases: &["communlink"],
        category: CommandCategory::Community,
        summary: "Unlink subgroup from Community",
        usage: "communityunlink <COMMUNITY_JID> <GROUP_JID>",
        example: "communityunlink 120363028123456789@g.us 120363099999999999@g.us",
        min_args: 2,
        missing_hint: "<COMMUNITY_JID> <GROUP_JID>",
    },
    CommandSpec {
        name: "communitysubgroups",
        aliases: &["commsubgroups"],
        category: CommandCategory::Community,
        summary: "Fetch all linked subgroups in Community",
        usage: "communitysubgroups <COMMUNITY_JID>",
        example: "communitysubgroups 120363028123456789@g.us",
        min_args: 1,
        missing_hint: "<COMMUNITY_JID>",
    },
    CommandSpec {
        name: "communityinfo",
        aliases: &["comminfo"],
        category: CommandCategory::Community,
        summary: "Inspect community metadata and permissions",
        usage: "communityinfo <COMMUNITY_JID>",
        example: "communityinfo 120363028123456789@g.us",
        min_args: 1,
        missing_hint: "<COMMUNITY_JID>",
    },
    CommandSpec {
        name: "communityinvite",
        aliases: &["comminvite"],
        category: CommandCategory::Community,
        summary: "Get Community invite link code",
        usage: "communityinvite <COMMUNITY_JID>",
        example: "communityinvite 120363028123456789@g.us",
        min_args: 1,
        missing_hint: "<COMMUNITY_JID>",
    },
    CommandSpec {
        name: "communityjoin",
        aliases: &["commjoin"],
        category: CommandCategory::Community,
        summary: "Join Community using invite code",
        usage: "communityjoin <INVITE_CODE>",
        example: "communityjoin COMM123456",
        min_args: 1,
        missing_hint: "<INVITE_CODE>",
    },
    CommandSpec {
        name: "communityleave",
        aliases: &["commleave"],
        category: CommandCategory::Community,
        summary: "Leave a WhatsApp Community",
        usage: "communityleave <COMMUNITY_JID>",
        example: "communityleave 120363028123456789@g.us",
        min_args: 1,
        missing_hint: "<COMMUNITY_JID>",
    },
    CommandSpec {
        name: "communityparticipating",
        aliases: &["commlist"],
        category: CommandCategory::Community,
        summary: "List all participating communities",
        usage: "communityparticipating",
        example: "communityparticipating",
        min_args: 0,
        missing_hint: "",
    },

    // ─── Commerce ─────────────────────────────────────────────────────────────
    CommandSpec {
        name: "catalog",
        aliases: &[],
        category: CommandCategory::Commerce,
        summary: "Fetch product catalog for a WhatsApp Business JID",
        usage: "catalog <BIZ_JID> [LIMIT]",
        example: "catalog 62812345678@s.whatsapp.net 20",
        min_args: 1,
        missing_hint: "<BIZ_JID>",
    },
    CommandSpec {
        name: "product",
        aliases: &[],
        category: CommandCategory::Commerce,
        summary: "Fetch single product details by product ID",
        usage: "product <BIZ_JID> <PRODUCT_ID>",
        example: "product 62812345678@s.whatsapp.net prod_123",
        min_args: 2,
        missing_hint: "<BIZ_JID> <PRODUCT_ID>",
    },
    CommandSpec {
        name: "collections",
        aliases: &[],
        category: CommandCategory::Commerce,
        summary: "Fetch catalog collections for a business JID",
        usage: "collections <BIZ_JID>",
        example: "collections 62812345678@s.whatsapp.net",
        min_args: 1,
        missing_hint: "<BIZ_JID>",
    },
    CommandSpec {
        name: "sendproduct",
        aliases: &[],
        category: CommandCategory::Commerce,
        summary: "Send product card message",
        usage: "sendproduct <TO> <BIZ> <PID> <TITLE> <PRICE> <CURR> [| CAPTION]",
        example: "sendproduct 62812345678@s.whatsapp.net 628999@s.whatsapp.net p1 \"Rose T-Shirt\" 150000 IDR",
        min_args: 6,
        missing_hint: "<TO> <BIZ> <PID> <TITLE> <PRICE> <CURR>",
    },
    CommandSpec {
        name: "sendorder",
        aliases: &[],
        category: CommandCategory::Commerce,
        summary: "Send commerce order message card",
        usage: "sendorder <TO> <SELLER> <ORDER_ID> <TITLE> <ITEM_COUNT> <TOTAL> <CURR> [| MSG]",
        example: "sendorder 62812345678@s.whatsapp.net 628999@s.whatsapp.net ord_1 \"Order 1\" 2 300000 IDR",
        min_args: 6,
        missing_hint: "<TO> <SELLER> <ORDER_ID> <TITLE> <ITEM_COUNT> <TOTAL> <CURR>",
    },
    CommandSpec {
        name: "orders",
        aliases: &[],
        category: CommandCategory::Commerce,
        summary: "List cached commerce orders from SQLite store",
        usage: "orders [CHAT_JID] [LIMIT]",
        example: "orders 62812345678@s.whatsapp.net 10",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "cachedcatalog",
        aliases: &[],
        category: CommandCategory::Commerce,
        summary: "List offline cached products from SQLite",
        usage: "cachedcatalog <BIZ_JID>",
        example: "cachedcatalog 62812345678@s.whatsapp.net",
        min_args: 1,
        missing_hint: "<BIZ_JID>",
    },
    CommandSpec {
        name: "cachedcollections",
        aliases: &[],
        category: CommandCategory::Commerce,
        summary: "List offline cached collections from SQLite",
        usage: "cachedcollections <BIZ_JID>",
        example: "cachedcollections 62812345678@s.whatsapp.net",
        min_args: 1,
        missing_hint: "<BIZ_JID>",
    },
    CommandSpec {
        name: "productdel",
        aliases: &[],
        category: CommandCategory::Commerce,
        summary: "Delete product(s) from catalog by ID",
        usage: "productdel <PRODUCT_ID1,PRODUCT_ID2...>",
        example: "productdel p1,p2,p3",
        min_args: 1,
        missing_hint: "<PRODUCT_IDS...>",
    },

    // ─── Privacy ──────────────────────────────────────────────────────────────
    CommandSpec {
        name: "privacy",
        aliases: &[],
        category: CommandCategory::Privacy,
        summary: "Fetch active privacy settings (last seen, profile pic, status, read receipts)",
        usage: "privacy",
        example: "privacy",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "privacyset",
        aliases: &[],
        category: CommandCategory::Privacy,
        summary: "Update privacy setting (last, profile, status, readreceipts, online, calladd)",
        usage: "privacyset <SETTING> <all|contacts|contact_blacklist|none>",
        example: "privacyset last none",
        min_args: 2,
        missing_hint: "<SETTING> <VALUE>",
    },
    CommandSpec {
        name: "blocklist",
        aliases: &[],
        category: CommandCategory::Privacy,
        summary: "Fetch server blocklist of blocked contacts",
        usage: "blocklist",
        example: "blocklist",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "blocklistlocal",
        aliases: &[],
        category: CommandCategory::Privacy,
        summary: "List cached blocked contacts from local SQLite",
        usage: "blocklistlocal",
        example: "blocklistlocal",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "block",
        aliases: &[],
        category: CommandCategory::Privacy,
        summary: "Block a contact JID",
        usage: "block <JID>",
        example: "block 62812345678@s.whatsapp.net",
        min_args: 1,
        missing_hint: "<JID>",
    },
    CommandSpec {
        name: "unblock",
        aliases: &[],
        category: CommandCategory::Privacy,
        summary: "Unblock a previously blocked contact JID",
        usage: "unblock <JID>",
        example: "unblock 62812345678@s.whatsapp.net",
        min_args: 1,
        missing_hint: "<JID>",
    },
    CommandSpec {
        name: "ghostmode",
        aliases: &["stealth"],
        category: CommandCategory::Privacy,
        summary: "Toggle Ghost Mode (suppress outbound read & story receipts)",
        usage: "ghostmode [on|off]",
        example: "ghostmode on",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "disappearing",
        aliases: &["ephemeral"],
        category: CommandCategory::Privacy,
        summary: "Configure disappearing / ephemeral messages timer for a chat",
        usage: "disappearing <JID> <off|24h|7d|90d|SECS>",
        example: "disappearing 62812345678@s.whatsapp.net 24h",
        min_args: 2,
        missing_hint: "<JID> <TIMER>",
    },

    // ─── WAProto ──────────────────────────────────────────────────────────────
    CommandSpec {
        name: "proto",
        aliases: &[],
        category: CommandCategory::WAProto,
        summary: "WAProto schema inspector, JSON reflection & live bundle extractor",
        usage: "proto <list|search|view|inspect|pull>",
        example: "proto search PollCreation",
        min_args: 1,
        missing_hint: "<list|search|view|inspect|pull>",
    },

    // ─── System & Diagnostics ─────────────────────────────────────────────────
    CommandSpec {
        name: "ping",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Check connection liveness to WhatsApp Web gateway",
        usage: "ping",
        example: "ping",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "status",
        aliases: &["info"],
        category: CommandCategory::System,
        summary: "Show connection status, active session JID, LID, and platform",
        usage: "status",
        example: "status",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "qr",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Display WhatsApp login QR code on demand",
        usage: "qr [large|raw]",
        example: "qr",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "pair",
        aliases: &["pairingcode"],
        category: CommandCategory::System,
        summary: "Request 8-digit WhatsApp phone pairing code",
        usage: "pair <PHONE_NUMBER>",
        example: "pair 628123456789",
        min_args: 1,
        missing_hint: "<PHONE_NUMBER>",
    },
    CommandSpec {
        name: "logfilter",
        aliases: &["logs", "log"],
        category: CommandCategory::System,
        summary: "Configure event filtering & dynamic live tracing debug level",
        usage: "logfilter <debug|trace|all|events|errors|quiet>",
        example: "logfilter debug",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "chats",
        aliases: &[],
        category: CommandCategory::System,
        summary: "List saved conversations from SQLite session store",
        usage: "chats",
        example: "chats",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "history",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Fetch past message history for a chat from SQLite",
        usage: "history <JID> [LIMIT]",
        example: "history 62812345678@s.whatsapp.net 30",
        min_args: 1,
        missing_hint: "<JID>",
    },
    CommandSpec {
        name: "contacts",
        aliases: &[],
        category: CommandCategory::System,
        summary: "List synchronized address book contacts from SQLite",
        usage: "contacts",
        example: "contacts",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "syncstatus",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Show AppState and HistorySync archive synchronization progress",
        usage: "syncstatus",
        example: "syncstatus",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "profilepic",
        aliases: &["pfp"],
        category: CommandCategory::System,
        summary: "Download profile picture for a JID",
        usage: "profilepic <JID> [OUTPUT_PATH]",
        example: "profilepic 62812345678@s.whatsapp.net ./pfp.jpg",
        min_args: 1,
        missing_hint: "<JID>",
    },
    CommandSpec {
        name: "setpfp",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Upload and update self profile picture",
        usage: "setpfp <IMAGE_PATH>",
        example: "setpfp ./avatar.jpg",
        min_args: 1,
        missing_hint: "<IMAGE_PATH>",
    },
    CommandSpec {
        name: "removepfp",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Remove self profile picture",
        usage: "removepfp",
        example: "removepfp",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "presencesub",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Subscribe to presence and online/typing updates for a contact",
        usage: "presencesub <JID>",
        example: "presencesub 62812345678@s.whatsapp.net",
        min_args: 1,
        missing_hint: "<JID>",
    },
    CommandSpec {
        name: "presence",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Send self presence update (composing, recording, available)",
        usage: "presence <composing|recording|paused|available|unavailable> [JID]",
        example: "presence composing 62812345678@s.whatsapp.net",
        min_args: 1,
        missing_hint: "<composing|recording|paused|available|unavailable>",
    },
    CommandSpec {
        name: "onwa",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Check if phone numbers are registered on WhatsApp",
        usage: "onwa <PHONE_NUMBERS...>",
        example: "onwa 62812345678,62898765432",
        min_args: 1,
        missing_hint: "<PHONE_NUMBERS...>",
    },
    CommandSpec {
        name: "calllink",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Generate a WhatsApp audio or video call link",
        usage: "calllink [video|audio]",
        example: "calllink video",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "calls",
        aliases: &[],
        category: CommandCategory::System,
        summary: "List incoming / outgoing VoIP call history logs from SQLite",
        usage: "calls [LIMIT]",
        example: "calls 20",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "callreject",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Reject an incoming WhatsApp call by call ID",
        usage: "callreject <CALL_ID> <FROM_JID>",
        example: "callreject 3EB0CALL123 62812345678@s.whatsapp.net",
        min_args: 2,
        missing_hint: "<CALL_ID> <FROM_JID>",
    },
    CommandSpec {
        name: "labels",
        aliases: &[],
        category: CommandCategory::System,
        summary: "List WhatsApp Business chat & message labels",
        usage: "labels",
        example: "labels",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "labeladd",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Create a new WhatsApp Business label",
        usage: "labeladd <NAME> [| COLOR_HEX]",
        example: "labeladd \"VIP Client\" | #FF0055",
        min_args: 1,
        missing_hint: "<NAME>",
    },
    CommandSpec {
        name: "labeldel",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Delete a WhatsApp Business label by ID",
        usage: "labeldel <LABEL_ID>",
        example: "labeldel 1",
        min_args: 1,
        missing_hint: "<LABEL_ID>",
    },
    CommandSpec {
        name: "labelchat",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Tag a chat with a label",
        usage: "labelchat <JID> <LABEL_ID>",
        example: "labelchat 62812345678@s.whatsapp.net 1",
        min_args: 2,
        missing_hint: "<JID> <LABEL_ID>",
    },
    CommandSpec {
        name: "unlabelchat",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Remove label tag from a chat",
        usage: "unlabelchat <JID> <LABEL_ID>",
        example: "unlabelchat 62812345678@s.whatsapp.net 1",
        min_args: 2,
        missing_hint: "<JID> <LABEL_ID>",
    },
    CommandSpec {
        name: "labelmsg",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Tag a specific message with a label",
        usage: "labelmsg <JID> <MSG_ID> <LABEL_ID>",
        example: "labelmsg 62812345678@s.whatsapp.net 3EB01234 1",
        min_args: 3,
        missing_hint: "<JID> <MSG_ID> <LABEL_ID>",
    },
    CommandSpec {
        name: "unlabelmsg",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Remove label from a specific message",
        usage: "unlabelmsg <JID> <MSG_ID> <LABEL_ID>",
        example: "unlabelmsg 62812345678@s.whatsapp.net 3EB01234 1",
        min_args: 3,
        missing_hint: "<JID> <MSG_ID> <LABEL_ID>",
    },
    CommandSpec {
        name: "chatlabels",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Show labels associated with a chat",
        usage: "chatlabels <JID>",
        example: "chatlabels 62812345678@s.whatsapp.net",
        min_args: 1,
        missing_hint: "<JID>",
    },
    CommandSpec {
        name: "labeledchats",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Show all chats tagged with a label ID",
        usage: "labeledchats <LABEL_ID>",
        example: "labeledchats 1",
        min_args: 1,
        missing_hint: "<LABEL_ID>",
    },
    CommandSpec {
        name: "reachoutstatus",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Query account reachout timelock / spam standing",
        usage: "reachoutstatus",
        example: "reachoutstatus",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "newchatcap",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Query new chat message cap quota",
        usage: "newchatcap",
        example: "newchatcap",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "cleandirty",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Clean server dirty bits (account_sync, groups)",
        usage: "cleandirty <account_sync|groups>",
        example: "cleandirty account_sync",
        min_args: 1,
        missing_hint: "<account_sync|groups>",
    },
    CommandSpec {
        name: "botlist",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Fetch WhatsApp AI Bot list v2",
        usage: "botlist",
        example: "botlist",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "usynclid",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Resolve WhatsApp LID from phone number via USync",
        usage: "usynclid <PHONE_NUMBER1,PHONE_NUMBER2...>",
        example: "usynclid 62812345678",
        min_args: 1,
        missing_hint: "<PHONE_NUMBERS...>",
    },
    CommandSpec {
        name: "usyncpn",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Resolve phone number from WhatsApp LID via USync",
        usage: "usyncpn <LID1,LID2...>",
        example: "usyncpn 123456789@lid",
        min_args: 1,
        missing_hint: "<LIDS...>",
    },
    CommandSpec {
        name: "usyncstatus",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Fetch contact text status & timestamp via USync",
        usage: "usyncstatus <JID1,JID2...>",
        example: "usyncstatus 62812345678@s.whatsapp.net",
        min_args: 1,
        missing_hint: "<JIDS...>",
    },
    CommandSpec {
        name: "devices",
        aliases: &[],
        category: CommandCategory::System,
        summary: "List paired companion devices (self or target JID)",
        usage: "devices [JID]",
        example: "devices",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "deviceremove",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Revoke/unlink companion device by ID",
        usage: "deviceremove <DEVICE_ID>",
        example: "deviceremove 1",
        min_args: 1,
        missing_hint: "<DEVICE_ID>",
    },
    CommandSpec {
        name: "mediaretry",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Request re-upload CDN URL for expired media message",
        usage: "mediaretry <MSG_ID>",
        example: "mediaretry 3EB01234",
        min_args: 1,
        missing_hint: "<MSG_ID>",
    },
    CommandSpec {
        name: "urlpreview",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Extract OpenGraph rich metadata preview from URL in text",
        usage: "urlpreview <TEXT_OR_URL>",
        example: "urlpreview https://rust-lang.org",
        min_args: 1,
        missing_hint: "<TEXT_OR_URL>",
    },
    CommandSpec {
        name: "cmdmode",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Toggle or view in-chat command runner execution mode",
        usage: "cmdmode [off|self|public]",
        example: "cmdmode public",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "telemetry",
        aliases: &["metrics"],
        category: CommandCategory::System,
        summary: "Show live performance metrics, latency, memory & stats",
        usage: "telemetry [reset]",
        example: "telemetry",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "selftest",
        aliases: &["test"],
        category: CommandCategory::System,
        summary: "Run automated self-tests across crypto, store, and protocol",
        usage: "selftest",
        example: "selftest",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "appstatesync",
        aliases: &["syncd", "syncappstate"],
        category: CommandCategory::System,
        summary: "Synchronize multi-device App State collections (mute, pin, archive, contacts, star)",
        usage: "appstatesync [all | critical | regular]",
        example: "appstatesync all",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "help",
        aliases: &["?", "h"],
        category: CommandCategory::System,
        summary: "Display contextual help, command guide, or category list",
        usage: "help [category | command | query]",
        example: "help sendbuttons",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "clear",
        aliases: &["cls"],
        category: CommandCategory::System,
        summary: "Clear the terminal screen",
        usage: "clear",
        example: "clear",
        min_args: 0,
        missing_hint: "",
    },
    CommandSpec {
        name: "exit",
        aliases: &["quit", "q"],
        category: CommandCategory::System,
        summary: "Gracefully disconnect and exit the REPL",
        usage: "exit",
        example: "exit",
        min_args: 0,
        missing_hint: "",
    },
];

/// Computes Levenshtein distance between two strings for typo correction.
#[allow(clippy::needless_range_loop)]
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let (m, n) = (a_chars.len(), b_chars.len());

    let mut dp = vec![vec![0; n + 1]; m + 1];

    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    dp[m][n]
}

/// Finds a command specification by name or alias (case-insensitive).
pub fn find_command(name: &str) -> Option<&'static CommandSpec> {
    let needle = name.trim().to_lowercase();
    COMMANDS.iter().find(|cmd| {
        cmd.name.eq_ignore_ascii_case(&needle)
            || cmd
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(&needle))
    })
}

/// Finds the closest matching command name for suggestions ("Did you mean?").
pub fn suggest_command(input: &str) -> Option<&'static str> {
    let needle = input.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }

    let mut best_match: Option<(&'static str, usize)> = None;

    for cmd in COMMANDS {
        let dist = levenshtein_distance(&needle, cmd.name);
        let max_dist = if needle.len() <= 3 {
            1
        } else if needle.len() <= 6 {
            2
        } else {
            3
        };

        if dist <= max_dist {
            match best_match {
                Some((_, current_min)) if dist < current_min => {
                    best_match = Some((cmd.name, dist));
                }
                None => {
                    best_match = Some((cmd.name, dist));
                }
                _ => {}
            }
        }
    }

    best_match.map(|(name, _)| name)
}

/// Formats a rich, user-friendly missing argument error with syntax and example.
pub fn format_missing_args(spec: &CommandSpec, missing_part: &str) -> String {
    let mut out = format!(
        "[-] Missing required argument for '{}': {}\n",
        spec.name, missing_part
    );
    out.push_str(&format!("    Usage:   {}\n", spec.usage));
    out.push_str(&format!("    Example: {}", spec.example));
    out
}

/// Prints contextual help: overview, specific category, or dedicated command card.
pub fn print_help(query_opt: Option<&str>) {
    let query = query_opt.unwrap_or("").trim();

    if query.is_empty() {
        print_general_help();
        return;
    }

    // 1. Check if query matches a category
    if let Some(cat) = CommandCategory::from_str(query) {
        print_category_help(cat);
        return;
    }

    // 2. Check if query matches a specific command
    if let Some(spec) = find_command(query) {
        print_command_detail(spec);
        return;
    }

    // 3. Search commands by substring in name or summary
    let matches: Vec<&CommandSpec> = COMMANDS
        .iter()
        .filter(|cmd| {
            cmd.name.to_lowercase().contains(&query.to_lowercase())
                || cmd.summary.to_lowercase().contains(&query.to_lowercase())
        })
        .collect();

    if !matches.is_empty() {
        println!(
            "\n=== Commands matching '{}' ({}) ===",
            query,
            matches.len()
        );
        for cmd in matches {
            println!("  {:<24} - {}", cmd.name, cmd.summary);
            println!("    Usage: {}", cmd.usage);
        }
        println!("\nTip: Type 'help <command>' for full syntax and examples.");
        return;
    }

    // 4. Fallback: typo suggestion
    if let Some(suggestion) = suggest_command(query) {
        println!(
            "[-] Unknown command or category: '{}'. Did you mean '{}'?",
            query, suggestion
        );
        println!("    Type 'help {}' to inspect that command.", suggestion);
    } else {
        println!(
            "[-] Unknown command or category: '{}'. Type 'help' for command list.",
            query
        );
    }
}

fn print_general_help() {
    println!("\n============================================================");
    println!("             Rose-Baileys CLI Command Guide                ");
    println!("============================================================");
    println!("Explore commands by category: type 'help <category>'");
    println!("  • messaging   - Text, buttons, lists, polls, voice notes, rich media");
    println!("  • groups      - Group administration, member approval, invite links");
    println!("  • status      - Status stories (HD image, video, audio, link, text)");
    println!("  • channels    - WhatsApp Newsletters, broadcast, reactions, admins");
    println!("  • community   - Parent Communities, linked subgroups, settings");
    println!("  • commerce    - Product catalogs, orders, collections");
    println!("  • privacy     - Privacy settings, blocklist, ghost mode, disappearing");
    println!("  • proto       - WAProto schema intelligence & live bundle extractor");
    println!("  • system      - Connection, QR code, phone pairing, diagnostics\n");
    println!("Quick Reference (Frequently Used):");
    println!("  send <JID> <TEXT>            Send regular message");
    println!("  sendbuttons <JID> <BODY> |.. Send interactive action buttons");
    println!("  sendlist <JID> <TITLE> |..   Send interactive single-select list menu");
    println!("  poll <JID> <Q> | <OPT...>    Create an interactive poll");
    println!("  qr [large|raw]               Display login QR code on demand");
    println!("  pair <PHONE>                 Request 8-digit WhatsApp pairing code");
    println!("  logfilter <MODE>             Switch logs (debug, trace, all, events, quiet)");
    println!("  status                       View current connection & account identity");
    println!("  exit / quit                  Exit interactive REPL loop");
    println!("\nTip: Type 'help <command>' (e.g. 'help sendbuttons') for detailed parameters and copy-paste examples.");
}

fn print_category_help(category: CommandCategory) {
    let cat_commands: Vec<&CommandSpec> =
        COMMANDS.iter().filter(|c| c.category == category).collect();

    println!(
        "\n=== {} ({}) ===",
        category.display_name(),
        cat_commands.len()
    );
    for cmd in cat_commands {
        println!("  {:<24} - {}", cmd.name, cmd.summary);
        println!("    Usage: {}", cmd.usage);
    }
    println!("\nTip: Type 'help <command>' for specific copy-pasteable examples.");
}

fn print_command_detail(spec: &CommandSpec) {
    println!("\n============================================================");
    println!(" Command: {}", spec.name);
    println!("============================================================");
    if !spec.aliases.is_empty() {
        println!(" Aliases:  {}", spec.aliases.join(", "));
    }
    println!(" Category: {}", spec.category.display_name());
    println!(" Summary:  {}", spec.summary);
    println!(" Usage:    {}", spec.usage);
    println!(" Example:  {}", spec.example);
    println!("------------------------------------------------------------\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("send", "send"), 0);
        assert_eq!(levenshtein_distance("send", "sendd"), 1);
        assert_eq!(levenshtein_distance("sendbutton", "sendbuttons"), 1);
        assert_eq!(levenshtein_distance("status", "statuss"), 1);
        assert_eq!(levenshtein_distance("poll", "pool"), 1);
    }

    #[test]
    fn test_find_command_by_name_and_alias() {
        assert!(find_command("send").is_some());
        assert_eq!(find_command("send").unwrap().name, "send");

        assert!(find_command("msg").is_some());
        assert_eq!(find_command("msg").unwrap().name, "send");

        assert!(find_command("sendbuttons").is_some());
        assert!(find_command("sendlist").is_some());
        assert!(find_command("downloadmedia").is_some());
        assert_eq!(find_command("downloadmedia").unwrap().name, "downloadmedia");
        assert!(find_command("download").is_some());
        assert_eq!(find_command("download").unwrap().name, "downloadmedia");
        assert!(find_command("sendmedia").is_some());
        assert_eq!(find_command("sendmedia").unwrap().name, "sendmedia");
        assert!(find_command("logfilter").is_some());
        assert!(find_command("nonexistent").is_none());
    }

    #[test]
    fn test_suggest_command_typo() {
        assert_eq!(suggest_command("sendbutton"), Some("sendbuttons"));
        assert_eq!(suggest_command("statuss"), Some("status"));
        assert_eq!(suggest_command("logfilte"), Some("logfilter"));
    }

    #[test]
    fn test_format_missing_args() {
        let spec = find_command("sendbuttons").unwrap();
        let err = format_missing_args(spec, "<JID> <BODY> | <BUTTONS...>");
        assert!(err.contains("Missing required argument for 'sendbuttons'"));
        assert!(err.contains("Usage:"));
        assert!(err.contains("Example:"));
    }

    #[test]
    fn test_command_category_lookup() {
        assert_eq!(
            CommandCategory::from_str("messaging"),
            Some(CommandCategory::Messaging)
        );
        assert_eq!(
            CommandCategory::from_str("groups"),
            Some(CommandCategory::Groups)
        );
        assert_eq!(
            CommandCategory::from_str("proto"),
            Some(CommandCategory::WAProto)
        );
        assert_eq!(CommandCategory::from_str("unknown"), None);
    }
}
