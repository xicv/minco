use async_trait::async_trait;
use aws_sdk_cognitoidentityprovider::types::{
    AttributeType, DeliveryMediumType, MessageActionType, UserType,
};
use minco_plugin_identity::{
    IdentityAdministrator, IdentityError, InviteIdentity, ManagedIdentity,
    validate_managed_username,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct CognitoIdentityAdministrator {
    client: aws_sdk_cognitoidentityprovider::Client,
    user_pool_id: String,
}

impl CognitoIdentityAdministrator {
    pub fn new(
        client: aws_sdk_cognitoidentityprovider::Client,
        user_pool_id: impl Into<String>,
    ) -> Result<Self, IdentityError> {
        let user_pool_id = user_pool_id.into();
        if user_pool_id.trim().is_empty()
            || user_pool_id.len() > 128
            || user_pool_id.chars().any(char::is_control)
        {
            return Err(IdentityError::InvalidAdministrationRequest(
                "Cognito user-pool ID is invalid".into(),
            ));
        }
        Ok(Self {
            client,
            user_pool_id,
        })
    }
}

#[async_trait]
impl IdentityAdministrator for CognitoIdentityAdministrator {
    async fn invite(&self, command: InviteIdentity) -> Result<ManagedIdentity, IdentityError> {
        command.validate()?;
        let mut attributes = vec![attribute("email", &command.email)?];
        for (name, value) in &command.attributes {
            attributes.push(attribute(name, value)?);
        }
        let mut request = self
            .client
            .admin_create_user()
            .user_pool_id(&self.user_pool_id)
            .username(&command.username)
            .set_user_attributes(Some(attributes));
        if command.send_invitation {
            request = request.desired_delivery_mediums(DeliveryMediumType::Email);
        } else {
            request = request.message_action(MessageActionType::Suppress);
        }
        let output = request
            .send()
            .await
            .map_err(|error| IdentityError::Provider(format!("AdminCreateUser failed: {error}")))?;
        output
            .user()
            .map(managed_from_user)
            .transpose()?
            .ok_or_else(|| IdentityError::Provider("AdminCreateUser returned no user".into()))
    }

    async fn get(&self, username: &str) -> Result<Option<ManagedIdentity>, IdentityError> {
        validate_managed_username(username)?;
        match self
            .client
            .admin_get_user()
            .user_pool_id(&self.user_pool_id)
            .username(username)
            .send()
            .await
        {
            Ok(output) => Ok(Some(ManagedIdentity {
                username: output.username().to_owned(),
                enabled: output.enabled(),
                status: output
                    .user_status()
                    .map_or_else(|| "UNKNOWN".into(), |status| status.as_str().to_owned()),
                attributes: attribute_map(output.user_attributes()),
            })),
            Err(error) if error
                .as_service_error()
                .is_some_and(
                    aws_sdk_cognitoidentityprovider::operation::admin_get_user::AdminGetUserError::is_user_not_found_exception,
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(IdentityError::Provider(format!(
                "AdminGetUser failed: {error}"
            ))),
        }
    }

    async fn disable(&self, username: &str) -> Result<bool, IdentityError> {
        validate_managed_username(username)?;
        match self
            .client
            .admin_disable_user()
            .user_pool_id(&self.user_pool_id)
            .username(username)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(error) if error
                .as_service_error()
                .is_some_and(
                    aws_sdk_cognitoidentityprovider::operation::admin_disable_user::AdminDisableUserError::is_user_not_found_exception,
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(IdentityError::Provider(format!(
                "AdminDisableUser failed: {error}"
            ))),
        }
    }

    async fn delete(&self, username: &str) -> Result<bool, IdentityError> {
        validate_managed_username(username)?;
        match self
            .client
            .admin_delete_user()
            .user_pool_id(&self.user_pool_id)
            .username(username)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(error) if error
                .as_service_error()
                .is_some_and(
                    aws_sdk_cognitoidentityprovider::operation::admin_delete_user::AdminDeleteUserError::is_user_not_found_exception,
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(IdentityError::Provider(format!(
                "AdminDeleteUser failed: {error}"
            ))),
        }
    }
}

fn attribute(name: &str, value: &str) -> Result<AttributeType, IdentityError> {
    AttributeType::builder()
        .name(name)
        .value(value)
        .build()
        .map_err(|error| IdentityError::InvalidAdministrationRequest(error.to_string()))
}

fn managed_from_user(user: &UserType) -> Result<ManagedIdentity, IdentityError> {
    let username = user
        .username()
        .ok_or_else(|| IdentityError::Provider("Cognito user has no username".into()))?;
    Ok(ManagedIdentity {
        username: username.to_owned(),
        enabled: user.enabled(),
        status: user
            .user_status()
            .map_or_else(|| "UNKNOWN".into(), |status| status.as_str().to_owned()),
        attributes: attribute_map(user.attributes()),
    })
}

fn attribute_map(attributes: &[AttributeType]) -> BTreeMap<String, String> {
    attributes
        .iter()
        .filter_map(|attribute| {
            attribute
                .value()
                .map(|value| (attribute.name().to_owned(), value.to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cognito_attributes_preserve_provider_names_without_secrets() {
        let attributes = vec![
            attribute("email", "person@example.com").unwrap(),
            attribute("custom:department", "engineering").unwrap(),
        ];
        let mapped = attribute_map(&attributes);
        assert_eq!(mapped["custom:department"], "engineering");
    }

    #[test]
    fn direct_adapter_commands_share_provider_neutral_validation() {
        let command = InviteIdentity {
            username: "person\nadmin".into(),
            email: "not-an-email".into(),
            attributes: BTreeMap::new(),
            send_invitation: false,
        };
        assert!(command.validate().is_err());
        assert!(validate_managed_username("person\nadmin").is_err());
    }
}
