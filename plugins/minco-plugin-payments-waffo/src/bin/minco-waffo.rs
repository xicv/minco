use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use clap::{Parser, Subcommand, error::ErrorKind};
use minco_config::{
    ConfigLayer, ConfigSourceKind, ConfigurationGraph, ConfigurationSchema, Environment,
    EnvironmentClass, SecretProvider, SecretReference,
};
use minco_core::{ConfigurationField, Plugin as _};
use minco_plugin_idempotency::{IdempotencyService, MemoryIdempotencyStore};
use minco_plugin_payments_waffo::{
    Checkout, CreateCheckoutSessionRequest, REVIEWED_WAFFO_SDK_REVISION, SecretResolver,
    SecretValue, WaffoConfiguration, WaffoError, WaffoPlugin, WaffoService, validate_action_path,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, Read as _, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};
use uuid::Uuid;

const OUTPUT_SCHEMA: u32 = 1;
const MAX_CONFIG_BYTES: usize = 1024 * 1024;

#[derive(Debug, Parser)]
#[command(
    name = "minco-waffo",
    version,
    about = "Config-driven Waffo Pancake automation for Minco applications"
)]
struct Cli {
    /// Minco configuration document containing [values.plugins.payments-waffo].
    #[arg(
        long,
        env = "MINCO_WAFFO_CONFIG",
        default_value = "minco.waffo.toml",
        global = true
    )]
    config: PathBuf,

    /// Stable lowercase Minco environment name used in the configuration digest.
    #[arg(
        long,
        env = "MINCO_ENVIRONMENT",
        default_value = "waffo-cli",
        global = true
    )]
    environment_name: String,

    /// Emit compact single-line JSON instead of indented JSON.
    #[arg(long, global = true)]
    compact: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate configuration without resolving secrets or contacting Waffo.
    ConfigCheck,
    /// Resolve and parse configured signing keys without contacting Waffo.
    Doctor,
    /// Generate a provider-compatible idempotency key.
    IdempotencyKey,
    /// Execute an explicitly named Waffo action using a JSON body file or stdin.
    Action {
        #[arg(long)]
        path: String,
        #[arg(long, value_name = "FILE|-", default_value = "-")]
        body: PathBuf,
        #[arg(long)]
        idempotency_key: String,
    },
    /// Create a common hosted checkout directly from command-line flags.
    Checkout {
        #[arg(long)]
        product_id: String,
        #[arg(long, default_value = "AUD")]
        currency: String,
        #[arg(long)]
        return_to: Option<String>,
        #[arg(long)]
        buyer_email: Option<String>,
        #[arg(long)]
        order_reference: Option<String>,
        #[arg(long = "metadata", value_name = "KEY=VALUE", value_parser = parse_metadata_entry)]
        metadata: Vec<(String, String)>,
        #[arg(long)]
        idempotency_key: String,
    },
    /// Create a hosted checkout session from a typed JSON request file or stdin.
    CheckoutCreate {
        #[arg(long, value_name = "FILE|-", default_value = "-")]
        body: PathBuf,
        #[arg(long)]
        idempotency_key: String,
    },
    /// Execute a read-only GraphQL query and optional variables document.
    Graphql {
        #[arg(long, value_name = "FILE|-")]
        query: PathBuf,
        #[arg(long, value_name = "FILE|-")]
        variables: Option<PathBuf>,
    },
    /// Register the HTTP webhook declared in the configuration file.
    WebhookAdd {
        #[arg(long)]
        idempotency_key: String,
    },
    /// Verify an untouched raw webhook body and emit safe deduplication keys.
    WebhookVerify {
        #[arg(long, env = "WAFFO_SIGNATURE")]
        signature: String,
        #[arg(long, value_name = "FILE|-", default_value = "-")]
        body: PathBuf,
    },
}

fn parse_metadata_entry(value: &str) -> std::result::Result<(String, String), String> {
    let Some((key, value)) = value.split_once('=') else {
        return Err("metadata must use KEY=VALUE".into());
    };
    if key.trim().is_empty()
        || key.chars().any(char::is_control)
        || value.chars().any(char::is_control)
    {
        return Err(
            "metadata keys must be non-empty and metadata must not contain control characters"
                .into(),
        );
    }
    Ok((key.to_owned(), value.to_owned()))
}

#[derive(Debug)]
struct LoadedConfiguration {
    digest: String,
    environment_class: EnvironmentClass,
    service: WaffoService,
}

#[derive(Debug, Default, Clone, Copy)]
struct CliSecretResolver;

#[async_trait]
impl SecretResolver for CliSecretResolver {
    async fn resolve(&self, reference: &SecretReference) -> Result<SecretValue, WaffoError> {
        match reference {
            SecretReference::EnvironmentVariable { name } => env::var(name)
                .map(SecretValue::new)
                .map_err(|_| WaffoError::SecretResolution),
            SecretReference::SystemsManagerParameter { .. } => Err(WaffoError::SecretResolution),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Success<T> {
    schema: u32,
    ok: bool,
    command: &'static str,
    data: T,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Failure<'a> {
    schema: u32,
    ok: bool,
    error: FailureBody<'a>,
}

#[derive(Debug, Serialize)]
struct FailureBody<'a> {
    code: &'a str,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigReport {
    configuration_digest: String,
    provider_contract_revision: &'static str,
    environment_class: EnvironmentClass,
    waffo_environment: &'static str,
    api_base_url: String,
    private_key_provider: SecretProvider,
    webhook_public_key_provider: Option<SecretProvider>,
    production_writes_allowed: bool,
    webhook_configured: bool,
    webhook_registration_configured: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let compact = env::args_os().any(|argument| argument == "--compact");
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            let _ = error.print();
            return ExitCode::SUCCESS;
        }
        Err(_) => {
            let failure = Failure {
                schema: OUTPUT_SCHEMA,
                ok: false,
                error: FailureBody {
                    code: "minco_waffo.arguments",
                    message: "invalid command-line arguments".into(),
                },
            };
            let _ = write_json(io::stderr().lock(), &failure, compact);
            return ExitCode::FAILURE;
        }
    };
    let compact = cli.compact;
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let waffo_error = error
                .chain()
                .find_map(|cause| cause.downcast_ref::<WaffoError>());
            let failure = Failure {
                schema: OUTPUT_SCHEMA,
                ok: false,
                error: FailureBody {
                    code: waffo_error.map_or("minco_waffo.failed", WaffoError::code),
                    message: error.to_string(),
                },
            };
            let _ = write_json(io::stderr().lock(), &failure, compact);
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    validate_stdin_sources(&cli)?;
    if matches!(&cli.command, Command::IdempotencyKey) {
        return emit(
            "idempotency-key",
            json!({ "idempotencyKey": format!("minco_waffo_{}", Uuid::now_v7().simple()) }),
            cli.compact,
        );
    }

    let loaded = load_configuration(&cli.config, &cli.environment_name)?;
    let configuration = loaded.service.configuration();
    let resolver = CliSecretResolver;

    match cli.command {
        Command::ConfigCheck => emit(
            "config-check",
            ConfigReport {
                configuration_digest: loaded.digest.clone(),
                provider_contract_revision: REVIEWED_WAFFO_SDK_REVISION,
                environment_class: loaded.environment_class,
                waffo_environment: configuration.environment().as_str(),
                api_base_url: configuration.api_base_url().to_string(),
                private_key_provider: configuration.private_key_provider(),
                webhook_public_key_provider: configuration.webhook_public_key_provider(),
                production_writes_allowed: configuration.production_writes_allowed(),
                webhook_configured: configuration.webhook_public_key_provider().is_some(),
                webhook_registration_configured: configuration.webhook_registration_configured(),
            },
            cli.compact,
        ),
        Command::Doctor => {
            let _client = loaded
                .service
                .client(loaded.environment_class, &resolver)
                .await?;
            let webhook_key = if configuration.webhook_public_key_provider().is_some() {
                let _verifier = loaded
                    .service
                    .webhook_verifier(loaded.environment_class, &resolver)
                    .await?;
                "valid"
            } else {
                "not_configured"
            };
            emit(
                "doctor",
                json!({
                    "privateKey": "valid",
                    "webhookPublicKey": webhook_key,
                    "providerContacted": false
                }),
                cli.compact,
            )
        }
        Command::Action {
            path,
            body,
            idempotency_key,
        } => {
            validate_action_path(&path)?;
            if configuration.environment()
                == minco_plugin_payments_waffo::WaffoEnvironment::Production
            {
                return Err(WaffoError::GenericProductionActionDisabled.into());
            }
            let body = read_json(&body, configuration.request_max_bytes())?;
            let client = loaded
                .service
                .client(loaded.environment_class, &resolver)
                .await?;
            let result = client.action_value(&path, &body, &idempotency_key).await?;
            emit("action", result, cli.compact)
        }
        Command::Checkout {
            product_id,
            currency,
            return_to,
            buyer_email,
            order_reference,
            metadata,
            idempotency_key,
        } => {
            ensure_production_write_allowed(configuration)?;
            let mut checkout = Checkout::guest(product_id, currency);
            if let Some(value) = return_to {
                checkout = checkout.return_to(value);
            }
            if let Some(value) = buyer_email {
                checkout = checkout.buyer_email(value);
            }
            if let Some(value) = order_reference {
                checkout = checkout.order_reference(value);
            }
            for (key, value) in metadata {
                checkout = checkout.metadata(key, value);
            }
            let request = checkout.build()?;
            let client = loaded
                .service
                .client(loaded.environment_class, &resolver)
                .await?;
            let result = client
                .create_checkout_session(&request, &idempotency_key)
                .await?;
            emit("checkout", result, cli.compact)
        }
        Command::CheckoutCreate {
            body,
            idempotency_key,
        } => {
            ensure_production_write_allowed(configuration)?;
            let request = serde_json::from_value::<CreateCheckoutSessionRequest>(read_json(
                &body,
                configuration.request_max_bytes(),
            )?)
            .context("checkout request does not match the documented JSON contract")?;
            request.validate()?;
            let client = loaded
                .service
                .client(loaded.environment_class, &resolver)
                .await?;
            let result = client
                .create_checkout_session(&request, &idempotency_key)
                .await?;
            emit("checkout-create", result, cli.compact)
        }
        Command::Graphql { query, variables } => {
            let query = read_utf8(&query, configuration.request_max_bytes())?;
            let variables = variables.map_or_else(
                || Ok(json!({})),
                |path| read_json(&path, configuration.request_max_bytes()),
            )?;
            let result = loaded
                .service
                .graphql_query(loaded.environment_class, &resolver, &query, variables)
                .await?;
            emit("graphql", result, cli.compact)
        }
        Command::WebhookAdd { idempotency_key } => {
            ensure_production_write_allowed(configuration)?;
            let client = loaded
                .service
                .client(loaded.environment_class, &resolver)
                .await?;
            let result = client.add_configured_http_webhook(&idempotency_key).await?;
            emit("webhook-add", result, cli.compact)
        }
        Command::WebhookVerify { signature, body } => {
            let body = read_bounded(&body, configuration.webhook_max_bytes())?;
            let verifier = loaded
                .service
                .webhook_verifier(loaded.environment_class, &resolver)
                .await?;
            let result = verifier.verify(&signature, &body)?;
            emit("webhook-verify", result, cli.compact)
        }
        Command::IdempotencyKey => unreachable!("handled before configuration loading"),
    }
}

fn ensure_production_write_allowed(configuration: &WaffoConfiguration) -> Result<()> {
    if configuration.environment() == minco_plugin_payments_waffo::WaffoEnvironment::Production
        && !configuration.production_writes_allowed()
    {
        return Err(WaffoError::ProductionWritesDisabled.into());
    }
    Ok(())
}

fn validate_stdin_sources(cli: &Cli) -> Result<()> {
    let mut stdin_sources = usize::from(cli.config == Path::new("-"));
    stdin_sources += match &cli.command {
        Command::Action { body, .. }
        | Command::CheckoutCreate { body, .. }
        | Command::WebhookVerify { body, .. } => usize::from(body == Path::new("-")),
        Command::Graphql { query, variables } => {
            usize::from(query == Path::new("-"))
                + usize::from(variables.as_deref() == Some(Path::new("-")))
        }
        _ => 0,
    };
    if stdin_sources > 1 {
        bail!("stdin may be selected by at most one input");
    }
    Ok(())
}

fn load_configuration(path: &Path, environment_name: &str) -> Result<LoadedConfiguration> {
    let document = read_utf8(path, MAX_CONFIG_BYTES)?;
    let layer = ConfigLayer::from_toml(
        ConfigSourceKind::EnvironmentFile,
        display_path(path),
        &document,
    )
    .context("Waffo configuration file is not valid Minco TOML")?;
    let environment_class = layer
        .environment_class()
        .ok_or_else(|| anyhow!("configuration must declare environment_class"))?;
    let schema = ConfigurationSchema::try_from_fields(Vec::<ConfigurationField>::new())
        .context("could not create the Minco configuration schema")?
        .with_plugin_descriptors([WaffoPlugin.descriptor()])
        .context("could not add the Waffo plugin schema")?;
    let graph = ConfigurationGraph::compile(
        &schema,
        Environment::new(environment_name, environment_class),
        [layer],
    )
    .context("Waffo configuration failed typed validation")?;
    let configuration = WaffoConfiguration::from_graph(&graph)?;
    configuration.validate_environment_class(environment_class)?;
    let idempotency = IdempotencyService::new(
        Arc::new(MemoryIdempotencyStore::default()),
        chrono::TimeDelta::minutes(5),
    )
    .context("could not initialize Minco idempotency")?;

    Ok(LoadedConfiguration {
        digest: graph.digest().to_owned(),
        environment_class,
        service: WaffoService::new(configuration, Arc::new(idempotency)),
    })
}

fn read_json(path: &Path, max_bytes: usize) -> Result<Value> {
    serde_json::from_slice(&read_bounded(path, max_bytes)?)
        .context("input is not a valid JSON document")
}

fn read_utf8(path: &Path, max_bytes: usize) -> Result<String> {
    String::from_utf8(read_bounded(path, max_bytes)?).context("input must be UTF-8")
}

fn read_bounded(path: &Path, max_bytes: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if path == Path::new("-") {
        io::stdin()
            .lock()
            .take(
                u64::try_from(max_bytes)
                    .unwrap_or(u64::MAX)
                    .saturating_add(1),
            )
            .read_to_end(&mut bytes)
            .context("could not read stdin")?;
    } else {
        let metadata = fs::metadata(path)
            .with_context(|| format!("could not inspect {}", display_path(path)))?;
        if metadata.len() > u64::try_from(max_bytes).unwrap_or(u64::MAX) {
            bail!("input exceeds the configured byte limit");
        }
        bytes = fs::read(path).with_context(|| format!("could not read {}", display_path(path)))?;
    }
    if bytes.len() > max_bytes {
        bail!("input exceeds the configured byte limit");
    }
    Ok(bytes)
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn emit<T: Serialize>(command: &'static str, data: T, compact: bool) -> Result<()> {
    write_json(
        io::stdout().lock(),
        &Success {
            schema: OUTPUT_SCHEMA,
            ok: true,
            command,
            data,
        },
        compact,
    )
}

fn write_json(mut writer: impl Write, value: &impl Serialize, compact: bool) -> Result<()> {
    if compact {
        serde_json::to_writer(&mut writer, value)?;
    } else {
        serde_json::to_writer_pretty(&mut writer, value)?;
    }
    writer.write_all(b"\n")?;
    Ok(())
}
