use super::map_docs_policy_error;
use effigy_docs_policy::DocsPolicyError;

#[test]
fn map_docs_policy_error_preserves_user_facing_message_shape() {
    let err = map_docs_policy_error(DocsPolicyError::Message("bad docs policy".to_owned()));
    assert_eq!(err.to_string(), "bad docs policy");
}
