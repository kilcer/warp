use super::runner::{AndroidRunService, AppIdentity};

// ========== extract_quoted_value ==========

#[test]
fn extract_package_name_from_aapt_output() {
    let line = "package: name='com.example.app' versionCode='1'";
    let result = AndroidRunService::test_parse_aapt_badging(line);
    assert_eq!(result.unwrap().package_name, "com.example.app");
}

#[test]
fn extract_launchable_activity_from_aapt_output() {
    let line = "launchable-activity: name='com.example.MainActivity'  label='' icon=''";
    let result = AndroidRunService::test_parse_aapt_badging(line);
    assert_eq!(
        result.unwrap().launch_activity.unwrap(),
        "com.example.MainActivity"
    );
}

#[test]
fn parse_aapt_badging_both_fields() {
    let output = "\
package: name='com.example.app' versionCode='1' versionName='1.0'
launchable-activity: name='com.example.MainActivity'  label='' icon=''
some-other-line: foo='bar'";
    let identity = AndroidRunService::test_parse_aapt_badging(output).unwrap();
    assert_eq!(identity.package_name, "com.example.app");
    assert_eq!(identity.launch_activity.unwrap(), "com.example.MainActivity");
}

#[test]
fn parse_aapt_badging_missing_package_name() {
    let output = "launchable-activity: name='com.example.MainActivity'";
    let result = AndroidRunService::test_parse_aapt_badging(output);
    assert!(result.is_err());
}

#[test]
fn parse_aapt_badging_no_launchable_activity() {
    let output = "package: name='com.example.app' versionCode='1'";
    let identity = AndroidRunService::test_parse_aapt_badging(output).unwrap();
    assert_eq!(identity.package_name, "com.example.app");
    assert!(identity.launch_activity.is_none());
}

#[test]
fn extract_quoted_value_simple() {
    let line = "package: name='com.test' rest";
    let result = AndroidRunService::extract_quoted_value_test(line, "package: name=");
    assert_eq!(result, Some("com.test".to_string()));
}

#[test]
fn extract_quoted_value_missing_prefix() {
    let result = AndroidRunService::extract_quoted_value_test("wrong: format", "package: name=");
    assert_eq!(result, None);
}

#[test]
fn extract_quoted_value_no_closing_quote() {
    let result = AndroidRunService::extract_quoted_value_test("package: name='unclosed", "package: name=");
    // split('\'') yields ["", "unclosed"] → next() gives "unclosed"
    assert_eq!(result, Some("unclosed".to_string()));
}

// ========== AppIdentity / launch logic ==========

#[test]
fn app_identity_with_activity() {
    let id = AppIdentity {
        package_name: "com.example".to_string(),
        launch_activity: Some("com.example.MainActivity".to_string()),
    };
    assert_eq!(id.package_name, "com.example");
    assert_eq!(id.launch_activity.as_deref().unwrap(), "com.example.MainActivity");
}

#[test]
fn app_identity_without_activity() {
    let id = AppIdentity {
        package_name: "com.example".to_string(),
        launch_activity: None,
    };
    assert!(id.launch_activity.is_none());
}
