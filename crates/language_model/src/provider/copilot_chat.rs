use std::sync::Arc;

use chrono::{NaiveDateTime, TimeDelta, Utc};
use copilot::copilot_chat::{
    request_api_token, stream_completion, ChatMessage, CopilotChat, Model as CopilotChatModel,
    Request as CopilotChatRequest, Role as CopilotChatRole,
};
use copilot::{Copilot, Status};
use futures::{FutureExt, StreamExt};
use gpui::{
    percentage, svg, Animation, AnimationExt, AppContext, AsyncAppContext, Model, ReadGlobal,
    Render, Subscription, Task, Transformation,
};
use http_client::HttpClient;
use settings::SettingsStore;
use std::time::Duration;
use strum::IntoEnumIterator;
use ui::{
    div, v_flex, Button, ButtonCommon, Clickable, Color, Context, FixedWidth, IconName,
    IconPosition, IconSize, IntoElement, Label, LabelCommon, ParentElement, Styled, ViewContext,
    VisualContext,
};

use crate::LanguageModelProviderState;
use crate::{
    LanguageModel, LanguageModelId, LanguageModelName, LanguageModelProvider,
    LanguageModelProviderId, LanguageModelProviderName, LanguageModelRequest, Role,
};

use super::open_ai::count_open_ai_tokens;

const PROVIDER_ID: &str = "copilot_chat";
const PROVIDER_NAME: &str = "GitHub Copilot Chat";

#[derive(Default, Clone, Debug, PartialEq)]
pub struct CopilotChatSettings {
    pub low_speed_timeout: Option<Duration>,
}

pub struct CopilotChatLanguageModelProvider {
    http_client: Arc<dyn HttpClient>,
    state: Model<State>,
}

pub struct State {
    oauth_token: Option<String>,
    api_key: Option<(String, NaiveDateTime)>,
    settings: CopilotChatSettings,
    _settings_subscription: Subscription,
    _chat_subscription: Subscription,
}

impl CopilotChatLanguageModelProvider {
    pub fn new(http_client: Arc<dyn HttpClient>, cx: &mut AppContext) -> Self {
        let state = cx.new_model(|cx| State {
            oauth_token: CopilotChat::global(cx).oauth_token.clone(),
            api_key: None,
            settings: CopilotChatSettings::default(),
            _settings_subscription: cx.observe_global::<SettingsStore>(|_, cx| {
                cx.notify();
            }),
            _chat_subscription: cx.observe_global::<CopilotChat>(|this: &mut State, cx| {
                let chat = CopilotChat::global(cx);

                this.oauth_token.clone_from(&chat.oauth_token);
                cx.notify();
            }),
        });

        Self { http_client, state }
    }

    async fn get_new_api_token(
        cx: &mut AsyncAppContext,
        oauth_token: String,
        http_client: Arc<dyn HttpClient>,
        low_speed_timeout: Option<Duration>,
        state: &Model<State>,
    ) -> Result<String, anyhow::Error> {
        let (api_key, expires_at) =
            request_api_token(&oauth_token, http_client.clone(), low_speed_timeout).await?;

        cx.update_model(state, |state, cx| {
            state.api_key = Some((api_key.clone(), expires_at));
            cx.notify();
        })?;

        Ok(api_key)
    }
}

impl LanguageModelProviderState for CopilotChatLanguageModelProvider {
    fn subscribe<T: 'static>(&self, cx: &mut gpui::ModelContext<T>) -> Option<gpui::Subscription> {
        Some(cx.observe(&self.state, |_, _, cx| {
            cx.notify();
        }))
    }
}

impl LanguageModelProvider for CopilotChatLanguageModelProvider {
    fn name(&self) -> crate::LanguageModelProviderName {
        LanguageModelProviderName(PROVIDER_NAME.into())
    }

    fn provided_models(&self, _cx: &AppContext) -> Vec<Arc<dyn crate::LanguageModel>> {
        CopilotChatModel::iter()
            .map(|model| {
                Arc::new(CopilotChatLanguageModel {
                    id: LanguageModelId::from(model.id().to_string()),
                    model,
                    state: self.state.clone(),
                    http_client: self.http_client.clone(),
                }) as Arc<dyn LanguageModel>
            })
            .collect()
    }

    fn is_authenticated(&self, cx: &AppContext) -> bool {
        self.state.read(cx).oauth_token.is_some()
    }

    fn authenticate(&self, cx: &AppContext) -> gpui::Task<gpui::Result<()>> {
        if self.is_authenticated(cx) {
            return Task::ready(Ok(()));
        } else {
            // We let the _chat_subscription deal with fetching an OAuth
            // token when necessary, so here we just need to provide a
            // helpful error message to the user
            let copilot = Copilot::global(cx).unwrap();

            match copilot.read(cx).status() {
                Status::Disabled => Task::ready(Err(anyhow::anyhow!("Copilot must be enabled for Copilot Chat to work. Please enable Copilot and try again."))),
                Status::Error(e) => Task::ready(Err(anyhow::anyhow!(format!("Received the following error while signing into Copilot: {e}")))),
                Status::Starting { task: _ } => Task::ready(Err(anyhow::anyhow!("Copilot is still starting, please wait for Copilot to start then try again"))),
                Status::Unauthorized => Task::ready(Err(anyhow::anyhow!("Unable to authorize with Copilot. Please make sure that you have an active Copilot and Copilot Chat subscription."))),
                Status::Authorized => Task::ready(Ok(())),
                Status::SignedOut => Task::ready(Err(anyhow::anyhow!("You have signed out of Copilot. Please sign in to Copilot and try again."))),
                Status::SigningIn { prompt: _ } => {
                    Task::ready(Err(anyhow::anyhow!("Still signing into Copilot...")))
                },
            }
        }
    }

    fn authentication_prompt(&self, cx: &mut ui::WindowContext) -> gpui::AnyView {
        cx.new_view(|cx| AuthenticationPrompt::new(cx)).into()
    }

    fn reset_credentials(&self, cx: &AppContext) -> gpui::Task<gpui::Result<()>> {
        if Copilot::global(cx).is_none() {
            return Task::ready(Err(anyhow::anyhow!(
                "Copilot is not available. Please ensure Copilot is enabled and running and try again."
            )));
        }

        let state = self.state.clone();
        let copilot = Copilot::global(cx).clone();

        cx.spawn(|mut cx| async move {
            cx.update_model(&copilot.unwrap(), |model, cx| model.sign_out(cx))?
                .await?;

            cx.update_model(&state, |this, cx| {
                this.oauth_token = None;
                this.api_key = None;
                cx.notify();
            })?;

            Ok(())
        })
    }

    fn id(&self) -> LanguageModelProviderId {
        LanguageModelProviderId(PROVIDER_ID.into())
    }
}

pub struct CopilotChatLanguageModel {
    id: LanguageModelId,
    model: CopilotChatModel,
    state: Model<State>,
    http_client: Arc<dyn HttpClient>,
}

impl LanguageModel for CopilotChatLanguageModel {
    fn id(&self) -> LanguageModelId {
        self.id.clone()
    }

    fn name(&self) -> crate::LanguageModelName {
        LanguageModelName::from(self.model.display_name().to_string())
    }

    fn provider_name(&self) -> LanguageModelProviderName {
        LanguageModelProviderName(PROVIDER_NAME.into())
    }

    fn telemetry_id(&self) -> String {
        format!("copilot_chat/{}", self.model.id())
    }

    fn max_token_count(&self) -> usize {
        self.model.max_token_count()
    }

    fn count_tokens(
        &self,
        request: crate::LanguageModelRequest,
        cx: &AppContext,
    ) -> futures::future::BoxFuture<'static, gpui::Result<usize>> {
        let model = match self.model {
            CopilotChatModel::Gpt4 => open_ai::Model::Four,
            CopilotChatModel::Gpt3_5Turbo => open_ai::Model::ThreePointFiveTurbo,
        };

        count_open_ai_tokens(request, model, cx)
    }

    fn stream_completion(
        &self,
        request: crate::LanguageModelRequest,
        cx: &AsyncAppContext,
    ) -> futures::future::BoxFuture<
        'static,
        gpui::Result<futures::stream::BoxStream<'static, gpui::Result<String>>>,
    > {
        if let Some(message) = request.messages.last() {
            if message.content.trim().is_empty() {
                const EMPTY_PROMPT_MSG: &str =
                    "Empty prompts aren't allowed. Please provide a non-empty prompt.";
                return futures::future::ready(Err(anyhow::anyhow!(EMPTY_PROMPT_MSG))).boxed();
            }

            // Copilot Chat has a restriction that the final message must be from the user.
            // While their API does return an error message for this, we can catch it earlier
            // and provide a more helpful error message.
            if !matches!(message.role, Role::User) {
                const USER_ROLE_MSG: &str = "The final message must be from the user. To provide a system prompt, you must provide the system prompt followed by a user prompt.";
                return futures::future::ready(Err(anyhow::anyhow!(USER_ROLE_MSG))).boxed();
            }
        }

        let state = self.state.clone();
        let http_client = self.http_client.clone();
        let request = self.to_copilot_chat_request(request);

        let Ok((oauth_token, api_key, low_speed_timeout)) =
            cx.read_model(&self.state, |state, _| {
                (
                    state.oauth_token.clone().unwrap(),
                    state.api_key.clone(),
                    state.settings.low_speed_timeout,
                )
            })
        else {
            return futures::future::ready(Err(anyhow::anyhow!("App state dropped"))).boxed();
        };

        cx.spawn(|mut cx| async move {

            let api_key = match api_key {
                Some((key, expires_at)) => {
                    if expires_at - Utc::now().naive_utc() < TimeDelta::minutes(5) {
                        CopilotChatLanguageModelProvider::get_new_api_token(&mut cx, oauth_token, http_client.clone(), low_speed_timeout, &state ).await?
                    } else {
                        key
                    }
                },
                None => CopilotChatLanguageModelProvider::get_new_api_token(&mut cx, oauth_token, http_client.clone(), low_speed_timeout, &state).await?
            };
            let response = stream_completion(
                http_client,
                api_key,
                request,
                low_speed_timeout,
            )
            .await?;
            let stream = response
                .filter_map(|response| async move {
                    match response {
                        Ok(result) => {
                            let choice = result.choices.first();
                            match choice {
                                Some(choice) => Some(Ok(choice.delta.content.clone().unwrap())),
                                None => Some(Err(anyhow::anyhow!(
                                    "The Copilot Chat API returned a response with no choices, but hadn't finished the message yet. Please try again."
                                ))),
                            }
                        }
                        Err(err) => Some(Err(err)),
                    }
                })
                .boxed();
            Ok(stream)
        })
        .boxed()
    }

    fn provider_id(&self) -> LanguageModelProviderId {
        LanguageModelProviderId(PROVIDER_ID.into())
    }
}

impl CopilotChatLanguageModel {
    pub fn to_copilot_chat_request(&self, request: LanguageModelRequest) -> CopilotChatRequest {
        CopilotChatRequest::new(
            self.model.clone(),
            request
                .messages
                .into_iter()
                .map(|msg| ChatMessage {
                    role: match msg.role {
                        Role::User => CopilotChatRole::User,
                        Role::Assistant => CopilotChatRole::Assistant,
                        Role::System => CopilotChatRole::System,
                    },
                    content: msg.content,
                })
                .collect(),
        )
    }
}

struct AuthenticationPrompt {
    copilot_status: Option<copilot::Status>,
    _subscription: Option<Subscription>,
}

impl AuthenticationPrompt {
    pub fn new(cx: &mut ViewContext<Self>) -> Self {
        let copilot = Copilot::global(cx);

        let _subscription = copilot.as_ref().map_or(None, |copilot| {
            Some(cx.observe(copilot, |this, model, cx| {
                this.copilot_status = Some(model.read(cx).status());
                cx.notify();
            }))
        });

        Self {
            copilot_status: copilot.map_or(None, |model| Some(model.read(cx).status())),
            _subscription,
        }
    }
}

impl Render for AuthenticationPrompt {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let loading_icon = svg()
            .size_8()
            .path(IconName::ArrowCircle.path())
            .text_color(cx.text_style().color)
            .with_animation(
                "icon_circle_arrow",
                Animation::new(Duration::from_secs(2)).repeat(),
                |svg, delta| svg.with_transformation(Transformation::rotate(percentage(delta))),
            );

        const ERROR_LABEL: &str = "Copilot Chat requires the Copilot plugin to be available and running. Please ensure Copilot is running and try again, or use a different Assistant provider.";
        match &self.copilot_status {
            Some(status) => match status {
                Status::Disabled => {
                    return v_flex().gap_6().p_4().child(Label::new(ERROR_LABEL));
                }
                Status::Starting { task: _ } => {
                    const LABEL: &str = "Starting Copilot...";
                    return v_flex()
                        .gap_6()
                        .p_4()
                        .justify_center()
                        .items_center()
                        .child(Label::new(LABEL))
                        .child(loading_icon);
                }
                Status::SigningIn { prompt: _ } => {
                    const LABEL: &str = "Signing in to Copilot...";
                    return v_flex()
                        .gap_6()
                        .p_4()
                        .justify_center()
                        .items_center()
                        .child(Label::new(LABEL))
                        .child(loading_icon);
                }
                Status::Error(_) => {
                    const LABEL: &str = "Copilot had issues starting. Please try restarting it. If the issue persists, try reinstalling Copilot.";
                    return v_flex()
                        .gap_6()
                        .p_4()
                        .child(Label::new(LABEL))
                        .child(svg().size_8().path(IconName::CopilotError.path()));
                }
                _ => {
                    const LABEL: &str =
                    "To use the assistant panel or inline assistant, you must login to GitHub Copilot. Your GitHub account must have an active Copilot Chat subscription.";
                    v_flex().gap_6().p_4().child(Label::new(LABEL)).child(
                        v_flex()
                            .gap_2()
                            .child(
                                Button::new("sign_in", "Sign In")
                                    .icon_color(Color::Muted)
                                    .icon(IconName::Github)
                                    .icon_position(IconPosition::Start)
                                    .icon_size(IconSize::Medium)
                                    .style(ui::ButtonStyle::Filled)
                                    .full_width()
                                    // I don't love that using the inline_completion_button module here, but it's the best way to share the logic between the two buttons,
                                    // without a bunch of refactoring.
                                    .on_click(|_, cx| {
                                        inline_completion_button::initiate_sign_in(cx)
                                    }),
                            )
                            .child(
                                div().flex().w_full().items_center().child(
                                    Label::new("Sign in to start using Github Copilot Chat.")
                                        .color(Color::Muted)
                                        .size(ui::LabelSize::Small),
                                ),
                            ),
                    )
                }
            },
            None => v_flex().gap_6().p_4().child(Label::new(ERROR_LABEL)),
        }
    }
}
