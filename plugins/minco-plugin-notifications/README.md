# minco-plugin-notifications

A narrow provider-neutral notification port for email, webhooks, in-app alerts,
and developer feedback notifications. Production applications inject a concrete
SES, webhook, queue, or other adapter. The memory sink supports deterministic
unit and feature tests.
