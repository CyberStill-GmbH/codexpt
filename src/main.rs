use std::{path::PathBuf, str::FromStr};

use chrono::{SecondsFormat, Utc};
use clap::{Parser, Subcommand};
use codexpt::{
    EvidenceRef, Message, MessageStatus, Store, TaskStatus, domain::MESSAGE_SCHEMA_VERSION,
    identity::IdentityStore,
};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "codexpt",
    version,
    about = "Local, auditable agent-collaboration PoC"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".codexpt/state.sqlite3")]
    db: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init,
    /// Create a consistent SQLite backup while the service is running.
    Backup {
        #[arg(long)]
        output: PathBuf,
    },
    /// Restore a backup to a new database path; the destination must not exist.
    Restore {
        #[arg(long)]
        backup: PathBuf,
    },
    Serve {
        #[arg(long, default_value_t = 4173)]
        port: u16,
    },
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    Message {
        #[command(subcommand)]
        command: MessageCommand,
    },
    Events {
        task_id: String,
    },
    Identity {
        #[command(subcommand)]
        command: IdentityCommand,
    },
}

#[derive(Debug, Subcommand)]
enum IdentityCommand {
    /// Explicit operator-only bootstrap for the first verified workspace Owner.
    BootstrapOwner {
        #[arg(long)]
        issuer: String,
        #[arg(long)]
        subject: String,
        #[arg(long)]
        email: String,
        #[arg(long)]
        workspace_name: String,
    },
}

#[derive(Debug, Subcommand)]
enum TaskCommand {
    List,
    Create {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long, default_value = "human:cesar")]
        actor: String,
    },
    Show {
        task_id: String,
    },
    Transition {
        task_id: String,
        #[arg(long, value_parser = parse_status)]
        expect: TaskStatus,
        #[arg(long, value_parser = parse_status)]
        to: TaskStatus,
        #[arg(long)]
        actor: String,
        #[arg(long, default_value = "")]
        correlation_id: String,
    },
    RequestDecision {
        task_id: String,
        #[arg(long)]
        question: String,
        #[arg(long, default_value = "agent:codex")]
        actor: String,
        #[arg(long, default_value = "")]
        correlation_id: String,
    },
    AnswerDecision {
        task_id: String,
        decision_id: String,
        #[arg(long)]
        answer: String,
        #[arg(long, default_value = "human:cesar")]
        actor: String,
        #[arg(long, default_value = "")]
        correlation_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum MessageCommand {
    Send {
        task_id: String,
        #[arg(long)]
        sender: String,
        #[arg(long)]
        recipient: String,
        #[arg(long)]
        body: String,
        #[arg(long, default_value = "")]
        correlation_id: String,
        #[arg(long)]
        parent_id: Option<String>,
        /// Evidence as LABEL|LOCATOR|SHA256; may be repeated.
        #[arg(long = "evidence")]
        evidence: Vec<String>,
        #[arg(long)]
        message_id: Option<String>,
    },
    List {
        task_id: String,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let db = cli.db;
    match cli.command {
        Command::Serve { port } => codexpt::web::serve(db, port).await?,
        Command::Backup { output } => {
            let store = Store::open(&db)?;
            store.backup_to(&output)?;
            println!("created consistent backup at {}", output.display());
        }
        Command::Restore { backup } => {
            Store::restore_from_backup(&backup, &db)?;
            let identity = IdentityStore::connect_sqlite(&db, 1).await?;
            let invalidated = identity.invalidate_auth_state_after_restore().await?;
            println!(
                "restored backup to {}; revoked {} sessions, consumed {} pending login transactions, and revoked {} pending invites",
                db.display(),
                invalidated.sessions_revoked,
                invalidated.login_transactions_consumed,
                invalidated.pending_invites_revoked,
            );
        }
        Command::Identity {
            command:
                IdentityCommand::BootstrapOwner {
                    issuer,
                    subject,
                    email,
                    workspace_name,
                },
        } => {
            let identity = IdentityStore::connect_sqlite(&db, 1).await?;
            let (user_id, workspace) = identity
                .bootstrap_first_owner(&issuer, &subject, &email, &workspace_name)
                .await?;
            print_json(&serde_json::json!({
                "user_id": user_id,
                "workspace_id": workspace.workspace_id,
                "workspace_name": workspace.name,
                "role": "OWNER",
                "note": "Bootstrap assigns Owner only to the exact operator-supplied OIDC issuer and subject."
            }))?;
        }
        command => {
            let mut store = Store::open(&db)?;
            match command {
                Command::Init => println!("initialized {}", db.display()),
                Command::Task { command } => match command {
                    TaskCommand::List => print_json(&store.list_tasks()?)?,
                    TaskCommand::Create {
                        title,
                        description,
                        actor,
                    } => print_json(&store.create_task(&title, &description, &actor)?)?,
                    TaskCommand::Show { task_id } => {
                        let task = store
                            .get_task(&task_id)?
                            .ok_or_else(|| format!("task {task_id} not found"))?;
                        print_json(&task)?;
                    }
                    TaskCommand::Transition {
                        task_id,
                        expect,
                        to,
                        actor,
                        correlation_id,
                    } => {
                        let correlation_id = nonempty_or_uuid(correlation_id);
                        print_json(&store.transition_task(
                            &task_id,
                            expect,
                            to,
                            &actor,
                            &correlation_id,
                        )?)?;
                    }
                    TaskCommand::RequestDecision {
                        task_id,
                        question,
                        actor,
                        correlation_id,
                    } => {
                        let decision_id = Uuid::new_v4().to_string();
                        let correlation_id = nonempty_or_uuid(correlation_id);
                        store.request_human_decision(
                            &task_id,
                            &decision_id,
                            &question,
                            &actor,
                            &correlation_id,
                        )?;
                        println!(
                            "{}",
                            serde_json::json!({"decision_id": decision_id, "status": "pending"})
                        );
                    }
                    TaskCommand::AnswerDecision {
                        task_id,
                        decision_id,
                        answer,
                        actor,
                        correlation_id,
                    } => {
                        let correlation_id = nonempty_or_uuid(correlation_id);
                        store.answer_human_decision(
                            &task_id,
                            &decision_id,
                            &answer,
                            &actor,
                            &correlation_id,
                        )?;
                        println!(
                            "{}",
                            serde_json::json!({"decision_id": decision_id, "status": "answered"})
                        );
                    }
                },
                Command::Message { command } => match command {
                    MessageCommand::Send {
                        task_id,
                        sender,
                        recipient,
                        body,
                        correlation_id,
                        parent_id,
                        evidence,
                        message_id,
                    } => {
                        let message = Message {
                            schema_version: MESSAGE_SCHEMA_VERSION,
                            message_id: message_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
                            task_id,
                            sender,
                            recipient,
                            correlation_id: nonempty_or_uuid(correlation_id),
                            parent_id,
                            status: MessageStatus::Recorded,
                            body,
                            evidence_refs: evidence
                                .iter()
                                .map(|value| parse_evidence(value))
                                .collect::<Result<_, _>>()?,
                            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                        };
                        let inserted = store.insert_message(&message)?;
                        println!(
                            "{}",
                            serde_json::json!({"message": message, "inserted": inserted})
                        );
                    }
                    MessageCommand::List { task_id } => {
                        print_json(&store.list_messages(&task_id)?)?
                    }
                },
                Command::Events { task_id } => print_json(&store.list_events(&task_id)?)?,
                Command::Serve { .. } => unreachable!("serve is handled before opening SQLite"),
                Command::Backup { .. } | Command::Restore { .. } => unreachable!(
                    "backup and restore are handled before opening the destination database"
                ),
                Command::Identity { .. } => {
                    unreachable!("identity commands are handled before opening the task store")
                }
            }
        }
    }
    Ok(())
}

fn parse_status(value: &str) -> Result<TaskStatus, String> {
    TaskStatus::from_str(value).map_err(|error| error.to_string())
}

fn parse_evidence(value: &str) -> Result<EvidenceRef, String> {
    let fields: Vec<_> = value.splitn(3, '|').collect();
    if fields.len() != 3 {
        return Err("evidence must be LABEL|LOCATOR|SHA256".into());
    }
    let evidence = EvidenceRef {
        label: fields[0].trim().into(),
        locator: fields[1].trim().into(),
        sha256: fields[2].trim().into(),
    };
    evidence.validate().map_err(|error| error.to_string())?;
    Ok(evidence)
}

fn nonempty_or_uuid(value: String) -> String {
    if value.trim().is_empty() {
        Uuid::new_v4().to_string()
    } else {
        value
    }
}

fn print_json(value: &impl serde::Serialize) -> Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
