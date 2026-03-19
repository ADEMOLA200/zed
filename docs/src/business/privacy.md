---
title: Privacy for Business - Zed Business
description: How Zed Business enforces data privacy across your organization, including auto-enforced prompt and training data protections.
---

# Privacy for Business

On individual Zed plans, privacy protections for AI data are opt-in: members choose whether to share data with Zed for product improvement. On Zed Business, these protections are enforced automatically for all members. No configuration required.

## What's enforced by default

For all members of a Zed Business organization:

- **No prompt sharing:** Member conversations and prompts are never shared with Zed. Members can't opt into [AI feedback via ratings](../ai/ai-improvement.md#ai-feedback-with-ratings), which would send conversation data to Zed.
- **No training data sharing:** Member code context is never shared with Zed for [Edit Prediction model training](../ai/ai-improvement.md#edit-predictions). Members can't opt in individually.

These protections are enforced server-side. They apply to every org member as soon as they join.

## How this differs from individual plans

On Free and Pro plans, data sharing is opt-in:

- Members can choose to rate AI responses, which shares that conversation with Zed.
- Members can opt into Edit Prediction training data collection for open source projects.

On Zed Business, neither option is available to members. These aren't configurable settings; they're enforced.

## What data still leaves the organization

These controls cover what Zed stores and trains on. They don't change how AI inference works.

When members use Zed's hosted AI models, their prompts and code context are sent to the relevant AI provider (Anthropic, OpenAI, Google, etc.) to generate responses. Zed requires zero-data retention agreements with these providers. See [AI Improvement](../ai/ai-improvement.md#data-retention-and-training) for details.

[Bring-your-own-key (BYOK)](../ai/llm-providers.md) and [external agents](../ai/external-agents.md) are governed by each provider's own terms; Zed doesn't control how they handle data.

## Additional controls for administrators

Administrators can go further using [Admin Controls](./admin-controls.md):

- Disable Zed-hosted models entirely, so no prompts reach Zed's model infrastructure
- Disable Edit Predictions org-wide
- Disable real-time collaboration

See [Admin Controls](./admin-controls.md) for the full list.
