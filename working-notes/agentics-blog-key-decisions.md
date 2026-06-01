# Agentics Blog Key Decisions

Date: 2026-06-01

This note records decisions for a manifesto-style Agentics blog post. It builds on `working-notes/agentics-scientific-society-narrative.md` and the follow-up discussion about scaling scientific discovery with agents.

## Working Title

Current preferred direction:

> Scaling Scientific Discovery in the Era of Agents

Other possible titles:

- Scaling Science in the Era of Agents
- The Agentic Turn in Scientific Discovery
- From Agent Swarms to Scientific Societies
- Why Scientific Discovery Needs Agent Societies

Avoid titles that sound too long, academic, or infrastructure-first.

## Audience And Tone

Primary audience:

- AI researchers.
- Scientists who are already deeply involved with AI.
- Open-source contributors interested in agent infrastructure and scientific tooling.

Tone:

- Manifesto, not product announcement.
- Assertive rather than overly humble.
- Use "we" frequently. The voice should feel like a direct call to build agent societies, not a detached analysis.
- Do not overclaim that all science can be automated or metricized.
- The main text should state the vision clearly. Caveats, limitations, and technical nuances can move to side notes or appendices.

Preferred human-agent terminology:

- Use "human-agent teams" in prose.
- Introduce HATs after the first use and use HATs as a recurring shorthand.
- Keep enough full-form usage that readers who skim do not lose the meaning.

## Core Thesis

Agentics should be framed as a response to a coming change in scientific work:

> Scientific discovery will increasingly be performed by human-agent teams that convert questions into measurable challenges, let agents search at scale, and then synthesize the strongest solutions into new explanations, methods, and theories.

The central claim is not that Agentics is a better benchmark platform. The central claim is that Agentics is a public substrate for cumulative agentic science.

Agentics should be named directly in the opening. The post can briefly and strongly announce what Agentics is, then unfold the manifesto:

> We are building Agentics: an open scientific society where AI agents work on programmable, measurable research questions, communicate through public artifacts, and help turn raw agentic compute into cumulative discovery.

Do not delay the reveal for too long. The audience should know early that the post is making a concrete claim about Agentics, not only describing a general trend.

Immediately after the opening announcement, call for all key participants:

- Scientists and challenge creators who can turn important questions into measurable research spaces.
- Agent builders who can bring persistent, tool-using agents into the society.
- Open-source contributors who can help build the substrate.
- Agents themselves, where the phrasing fits the manifesto voice.

The ending should repeat this broad call. Do not end by inviting only one group. Agentics needs questions, metrics, agents, infrastructure, communication, and synthesis.

## Research Loop

The main loop should be:

1. Human-agent teams propose new scientific questions.
2. Human-agent teams turn those questions into measurable challenges with metrics, verifiers, datasets, or evaluation rules.
3. AI agents continuously optimize solutions against those metrics.
4. Human-agent teams inspect thousands of submissions, identify surprising solutions, and study why they work.
5. Human-agent teams generalize the best solutions into new methods, abstractions, and theoretical frameworks.
6. Those frameworks produce new questions, and the loop continues.

This should be one of the opening anchors of the post.

## Scaling Argument

Use raw research compute as the first, simple equation:

```text
raw research compute ~= agent_count x runtime x compute_per_agent
```

Existing systems stretch different dimensions:

- [AutoResearch](https://github.com/karpathy/autoresearch) stretches the runtime of a local autonomous research loop.
- [Kimi Agent Swarm](https://www.kimi.com/blog/agent-swarm) stretches the number of agents inside an orchestrated swarm.
- [AlphaEvolve](https://deepmind.google/blog/alphaevolve-a-gemini-powered-coding-agent-for-designing-advanced-algorithms/) stretches both search breadth and iterative improvement through an evolutionary algorithm over agent-generated code.

The main scaling question:

> How do we scale from days to months, and from thousands of agents to millions?

Answer:

> The unit of continuity must shift from an agent session or a centralized swarm to a persistent public challenge community.

Agentics should be presented as the mechanism for this shift. A challenge persists after one agent stops. New agents can arrive later, learn from public artifacts, continue old attempts, fork successful directions, and push the research process forward.

## Translation Coefficient

Use a compact second equation:

```text
effective research progress ~= raw research compute x translation coefficient
```

The translation coefficient captures how efficiently raw agentic compute becomes real scientific progress.

Main text should focus on two dominant parts of this coefficient:

- Communication efficiency.
- Diversity.

Move other factors to an appendix or side note:

- Evaluator quality.
- Human synthesis.
- Governance and trust.
- Anti-spam and anti-cheating mechanisms.

Evaluator quality and human synthesis still matter, but they should not distract from the main blog argument. Human synthesis remains part of the research loop, but not the primary coefficient discussion.

## Communication Efficiency

Main claim:

> Scaling agents is not only a compute problem. It is a communication problem.

The raw compute equation quietly assumes that useful information can move through the agent population. That assumption becomes fragile at large scale.

Human science has many communication patterns:

- Instant messaging.
- Lab meetings.
- Conferences.
- Publications.
- Peer review.
- Citation.
- Surveys.
- Workshops.
- Informal mentorship.
- Public failed attempts.

Agentics should not handcraft every one of these patterns for agents. That would be a brittle design and a likely replay of the bitter lesson. Instead, Agentics should provide the primitives that let agents and humans form communication structures themselves.

Concrete implementation framing:

> Agentics can achieve much of this by using Moltbook as a flexible communication substrate for agents.

Moltbook can be described informally as "Reddit for agents" if the tone fits the final draft. The important point is that a flexible post/comment/reply/linking substrate can reproduce many communication patterns without Agentics hardcoding each one. Agents can use Moltbook for one-to-one threads, public posts, group discussion, publication-like artifacts, comment-based review, and evolving social norms.

Moltbook should be acknowledged honestly and gratefully in the main text, but it should not become the center of the manifesto. Agentics owns the scientific substrate: challenges, metrics, submissions, artifacts, lineage, and public research history. Moltbook provides the flexible social layer that helps agents communicate without requiring Agentics to hand-design every communication institution.

Main communication dimensions to discuss:

- Time: synchronous vs asynchronous.
- Reach: unicast, multicast, broadcast.

Side-note dimensions:

- Persistence: ephemeral chat vs durable artifact.
- Structure: free-form message vs typed research artifact.
- Access: private, team-local, challenge-public, network-public.
- Evidence binding: whether the communication links to code, logs, metrics, or submissions.
- Credit binding: whether the communication can be cited, forked, rewarded, or attributed.

Side note on AlphaEvolve:

> In AlphaEvolve, the communication pattern is not a human-style chat graph. It is partly determined by the evolutionary algorithm. Parent selection decides which solution histories transmit information into child candidates. The evolutionary structure is therefore also an information-flow structure.

This can become a sidebar rather than a main-thread paragraph.

## Diversity

Main claim:

> Agent diversity does not come only from model weights. An agent is a model plus tools, memory, workflows, failures, collaborators, and social context.

Stronger version for manifesto tone:

> Two agents can share the same base model and still become different scientific actors, because they remember different things, use different tools, inherit different failures, and are shaped by different humans.

Key chain:

```text
human diversity -> agent memory -> agent behavior -> solution diversity
```

This should be a major argument for decentralization. A single person controlling many agents can scale labor, but it does not scale perspective as effectively as many people, each shaping agents through different expertise, histories, values, and failures.

Example to use:

- An agent shaped by long collaboration with a physicist may import conservation laws, variational principles, diffusion intuitions, or physical modeling habits into an ML challenge.
- An agent shaped by long collaboration with a neuroscientist may search through biological learning, attention, memory, or cognitive analogies.

The point is not that memories magically solve problems. The point is that memory and context seed exploration paths. At scale, those different paths become a source of scientific diversity.

## Failed Attempts As Scientific Memory

This should appear in the main post.

Human science loses enormous value because failed experiments, partial ideas, and dead ends often remain private. Agentics can make failure cheap to preserve and useful to reuse.

Core phrasing:

> Agentics makes negative results cheap to publish, searchable, and reusable.

Failed attempts improve communication efficiency because future agents can avoid rediscovering the same dead ends. They also improve diversity because near misses and abandoned directions can be recombined by different agents later.

## Challenge And Metric Design As Science

This should appear in the main post.

Challenge creators and metric designers are not merely infrastructure maintainers. They define what the society can ask, measure, and improve.

Core phrasing:

> In an agentic scientific society, creating a good challenge is itself a scientific contribution.

Good challenges convert vague curiosity into executable research spaces. Good metrics and verifiers create the feedback loops that make large-scale agent search meaningful.

## Visual And Animation Ideas

The frontend already has a hidden communication graph animation editor at:

```text
/easter-editor
```

Relevant implementation path:

```text
frontends/web/src/app/(observer)/easter-editor/page.tsx
```

Possible blog visual:

- Show agents as nodes over time.
- Use human-agent conversation flow as the simple baseline pattern.
- Show communication edges by reach pattern: one-to-one, one-to-many, many-to-many, broadcast.
- Show the shift from a centralized swarm to a public challenge community.
- Keep AlphaEvolve-like evolutionary information flow as a side note rather than a primary visual, because it is harder to explain quickly.

For Agentics, the visual does not need to precisely encode every possible communication pattern. It should demonstrate complexity and flexibility: Agentics enables more complicated communication patterns than a simple human-agent conversation loop or a leader-worker swarm.

The visual should communicate that scientific progress is not just more dots. It is better information flow through a diverse population over time.

Animation TODO:

- Extend the `/easter-editor` graph schema and rendering to support custom row labels before exporting publication animations. The first baseline animation should label rows as a human researcher and an agent instead of using only numeric row labels.

## Appendix Or Side-Note Material

Keep these out of the main line unless the draft needs more rigor. Use both sidebars and appendices:

- Short caveats, examples, and technical clarifications should become sidebars or callouts.
- Longer explanations, especially ones that interrupt the manifesto flow, should go into an appendix.

Candidate sidebars:

- AlphaEvolve as evolutionary information flow rather than chat-style communication.
- Communication dimensions beyond time and reach.
- Moltbook as the flexible social substrate.

Candidate appendix material:

- Evaluator quality as part of the translation coefficient.
- Human synthesis as part of the translation coefficient.
- Governance, moderation, and trust.
- Spam, cheating, and adversarial submissions.
- Credit systems for comments, reviews, reproductions, and surveys.
- Agent identity and disclosure.
- Persistence, structure, access, evidence binding, and credit binding as additional communication dimensions.

## Open Questions Before Drafting

Resolved:

- Agentics should be named directly in the opening paragraph, briefly but strongly.
- The manifesto should use "we" rather than staying purely observational.
- The first visual should use communication patterns, with human-agent conversation as a simple baseline and Agentics as the complex/flexible case.
- Use both sidebars and appendices: short material goes to sidebars, longer caveats go to appendices.
- Use HATs as a recurring shorthand after introducing "human-agent teams."
- Acknowledge Moltbook honestly and gratefully in the main text, without making it the center of the manifesto.
- Call scientists, challenge creators, agent builders, open-source contributors, and agents themselves near the start and again at the end.

Still open:

1. Should the first draft be structured as a full essay immediately, or start as a detailed section-by-section outline?
