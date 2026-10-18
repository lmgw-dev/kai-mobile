use kai_mobile_core::{Colours, CoreError, Settings};

use super::rig::Rig;

#[tokio::test]
async fn settings_survive_a_restart() {
    let rig = Rig::with(|_, _| None, Settings::default()).await;
    assert_eq!(rig.core.settings(), Settings::default());

    let mut s = rig.core.settings();
    s.lmgw.timeout_s = 25;
    s.appearance.colours = Colours::System;
    s.voice.ptt_tail_ms = Some(300);
    rig.core.set_settings(s.clone()).await.unwrap();
    assert_eq!(rig.core.settings(), s);
    assert!(!rig.dir.path().join("settings.json.tmp").exists());

    let again = rig.restart();
    assert_eq!(again.core.settings(), s);
    assert_eq!(again.core.settings_notice(), None);
}

#[tokio::test]
async fn an_invalid_value_is_refused_and_leaves_the_old_settings() {
    let rig = Rig::with(|_, _| None, Settings::default()).await;
    let mut s = rig.core.settings();
    s.lmgw.timeout_s = 0;
    let err = rig.core.set_settings(s).await.unwrap_err();
    assert!(matches!(err, CoreError::InvalidSetting { ref key, .. } if key == "lmgw.timeout_s"));
    assert_eq!(rig.core.settings(), Settings::default());
}

#[tokio::test]
async fn a_corrupt_file_is_set_aside_not_deleted_and_the_notice_says_so() {
    let rig = Rig::with(
        |_, dir| {
            std::fs::write(dir.join("settings.json.seed"), "{ not json").unwrap();
            None
        },
        Settings::default(),
    )
    .await;
    // Overwrite the file the rig wrote and open again.
    std::fs::write(rig.dir.path().join("settings.json"), "{ not json").unwrap();
    let rig = rig.restart();
    assert_eq!(rig.core.settings(), Settings::default());
    let notice = rig.core.settings_notice().expect("a notice");
    assert!(notice.contains("settings.json.rejected-"), "{notice}");
    let aside = std::fs::read_dir(rig.dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("settings.json.rejected-")
        })
        .expect("the rejected file is kept");
    assert_eq!(std::fs::read_to_string(aside).unwrap(), "{ not json");
    assert!(!rig.dir.path().join("settings.json").exists());
}

#[tokio::test]
async fn the_about_page_facts() {
    let rig = Rig::unpaired().await;
    let about = rig.core.about();
    assert_eq!(about.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(about.lmgw_pin.commit.len(), 40);
}
