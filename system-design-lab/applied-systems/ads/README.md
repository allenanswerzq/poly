# Advertising Systems Deep Dive

## Overview

Online advertising is a ~$600B/year industry built on real-time systems that decide which ad to show a user in <100ms. Understanding ad systems means understanding real-time bidding, CTR prediction, auction theory, feature engineering at scale, and the feedback loops that make it all work.

## The Big Picture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     Ad Serving: End-to-End Flow                         │
│                                                                         │
│   User visits            Ad Request         Auction +           Ad      │
│   publisher page  ──►   to Ad Server  ──►  ML Ranking   ──►  Shown     │
│   (CNN, YouTube)         (<10ms)           (<50ms)          to user     │
│                                                                         │
│   User clicks/          Click/conversion    Billing +         Model     │
│   converts       ──►   event logged   ──►  Attribution  ──► Retrained  │
│   (or doesn't)          (streaming)        (batch + stream)  (hourly)   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘

The core loop:
  1. User arrives → predict which ad they'll engage with
  2. Run an auction → pick the winning ad
  3. Show the ad → observe what happens (click? purchase? ignore?)
  4. Log everything → retrain models → better predictions next time
```

## Types of Online Advertising

```
┌──────────────────┬────────────────────────┬─────────────────────────────┐
│ Type             │ How it works            │ Examples                    │
├──────────────────┼────────────────────────┼─────────────────────────────┤
│ Search ads       │ Bid on keywords         │ Google Ads, Bing Ads        │
│ Display ads      │ Banner/image on sites   │ Google Display Network      │
│ Social ads       │ In-feed on social apps  │ Meta (FB/IG), TikTok, X    │
│ Video ads        │ Pre-roll, mid-roll      │ YouTube, Twitch             │
│ Native ads       │ Looks like content      │ Taboola, Outbrain           │
│ Retail media     │ Ads on shopping sites   │ Amazon Sponsored, Walmart   │
│ Programmatic     │ Automated RTB exchange  │ The Trade Desk, DV360       │
└──────────────────┴────────────────────────┴─────────────────────────────┘
```

## 1. Real-Time Bidding (RTB) — How Programmatic Ads Work

```
When you load a webpage with an ad slot, this happens in ~100ms:

  ┌──────────┐      ┌──────────────┐      ┌──────────────────────────┐
  │ Publisher │      │ Ad Exchange  │      │ DSP (Demand-Side         │
  │ (CNN.com) │─────►│ (Google AdX, │─────►│  Platform)               │
  │           │  ①   │  OpenRTB)    │  ②   │ Represents advertisers   │
  └──────────┘      └──────┬───────┘      │ Decides bid amount       │
                           │               │ Runs ML models           │
                           │               └──────────┬───────────────┘
                           │                          │
                           │  ③ Bid responses          │
                           │◄─────────────────────────┘
                           │
                           │  ④ Auction: highest bid wins
                           │  ⑤ Winner's ad is served
                           ▼
                    ┌──────────────┐
                    │ User sees ad │
                    └──────────────┘

  ① Bid request: user info (cookie/device ID), page context, ad slot size
  ② DSPs have ~50-100ms to respond with a bid
  ③ Multiple DSPs bid simultaneously
  ④ Second-price or first-price auction
  ⑤ Winning ad creative rendered in the page

Scale: Google processes ~10 million ad auctions per second.
```

### Auction Types

```
Second-Price Auction (traditional, Vickrey):
  Bidder A: $5.00
  Bidder B: $3.00   ← A wins, pays $3.01 (second price + $0.01)
  Bidder C: $2.00

  Incentive: bid your true value (no need to game it)
  Google used this until 2019.

First-Price Auction (now standard):
  Bidder A: $5.00   ← A wins, pays $5.00 (their actual bid)
  Bidder B: $3.00
  Bidder C: $2.00

  Incentive: shade your bid below true value (bid $3.50 instead of $5)
  Requires "bid shading" algorithms to avoid overpaying.
  Industry switched because of header bidding transparency.

Revenue = Σ (winning_bid × impressions)
```

## 2. Ad Ranking — The Core ML Problem

### The Ranking Formula

```
Every major ad platform ranks ads by Expected Revenue:

  Score = pCTR × bid × quality_factor

  pCTR       = predicted click-through rate (ML model output, 0 to 1)
  bid        = advertiser's bid (dollars per click or per 1000 impressions)
  quality    = ad relevance, landing page quality, historical performance

Google calls this: Ad Rank = Bid × Quality Score
Meta calls this:  Total Value = Advertiser Bid × Estimated Action Rate + User Value

The ad with the highest score wins the auction.
```

### Multi-Stage Ranking (Funnel)

```
You can't run a heavy ML model on every ad candidate. Use a funnel:

  ┌─────────────────────────────────────────────────┐
  │  Stage 1: RETRIEVAL                              │
  │  Pool: ~10,000 candidate ads                     │
  │  Method: inverted index, keyword match,          │
  │          targeting rules (geo, age, interests)   │
  │  Latency budget: ~5ms                            │
  │  Output: ~1,000 candidates                       │
  ├─────────────────────────────────────────────────┤
  │  Stage 2: PRE-RANKING (lightweight model)        │
  │  Pool: ~1,000 candidates                         │
  │  Method: simple logistic regression or small NN  │
  │  Features: sparse (ad ID, user segment)          │
  │  Latency budget: ~10ms                           │
  │  Output: ~100 candidates                         │
  ├─────────────────────────────────────────────────┤
  │  Stage 3: RANKING (heavy model)                  │
  │  Pool: ~100 candidates                           │
  │  Method: deep neural network (DLRM, DCN, etc.)  │
  │  Features: dense + sparse + cross features       │
  │  Latency budget: ~30ms                           │
  │  Output: ~10 ranked ads                          │
  ├─────────────────────────────────────────────────┤
  │  Stage 4: AUCTION + POLICY                       │
  │  Apply business rules, frequency capping,        │
  │  ad quality filters, diversity constraints       │
  │  Pick winner(s), determine price                 │
  │  Output: 1-3 ads to show                         │
  └─────────────────────────────────────────────────┘
```

## 3. CTR Prediction — The Core ML Model

### Feature Engineering

```
The most important part. Features fall into categories:

┌─────────────────┬────────────────────────────────────────────────────┐
│ Category        │ Examples                                           │
├─────────────────┼────────────────────────────────────────────────────┤
│ User features   │ age, gender, location, device, past click history │
│                 │ user embedding (from historical behavior)          │
│ Ad features     │ ad ID, advertiser, creative type, category,       │
│                 │ landing page quality, historical CTR               │
│ Context         │ time of day, day of week, page/app context,       │
│                 │ position on page, ad slot size                     │
│ Cross features  │ user×ad (has user seen this ad before?),          │
│                 │ user×category (does user like this category?)      │
│ Sequence        │ user's last N actions (click stream)               │
└─────────────────┴────────────────────────────────────────────────────┘

Sparse features (categorical, high-cardinality):
  user_id:      10 billion unique values → embedding table
  ad_id:        100 million unique values → embedding table
  query×ad:     combinatorial explosion → hashed cross feature

Dense features (numerical):
  user_age, ad_historical_ctr, time_since_last_click, ...
```

### Model Evolution

```
Logistic Regression (2000s):
  Simple, interpretable, easy to serve.
  w · x + b → sigmoid → probability
  Used at Google for years. Still a strong baseline.

GBDT + LR (2014, Facebook):
  Gradient Boosted Trees create features → feed into LR.
  Trees find nonlinear interactions humans would miss.

Deep Learning era (2016+):

  Wide & Deep (Google, 2016):
    Wide path: memorization (logistic regression on cross features)
    Deep path: generalization (DNN on embeddings)
    Combined → best of both.

  DeepFM (2017):
    Factorization Machine layer (learns 2nd-order feature interactions)
    + Deep network (learns higher-order interactions)
    No manual feature engineering needed.

  DCN v2 — Deep & Cross Network (Google, 2020):
    Cross network explicitly models feature interactions
    at each layer: x_{l+1} = x_0 ⊙ (W_l · x_l + b_l) + x_l
    More expressive than FM, efficient.

  DLRM — Deep Learning Recommendation Model (Meta, 2019):
    Embedding tables for sparse features (the HUGE part)
    MLP for dense features
    Feature interactions via dot products
    This is what Meta actually uses. Embedding tables = TBs of parameters.

  ┌──────────────────────────────────────────────────────┐
  │  DLRM Architecture (Meta)                             │
  │                                                       │
  │  Sparse features    Dense features                    │
  │  (user_id, ad_id)   (age, time, ...)                  │
  │       │                    │                           │
  │       ▼                    ▼                           │
  │  ┌──────────┐        ┌──────────┐                     │
  │  │Embedding │        │ Bottom   │                     │
  │  │ Tables   │        │  MLP     │                     │
  │  │(TB-scale)│        │          │                     │
  │  └────┬─────┘        └────┬─────┘                     │
  │       │                    │                           │
  │       └──────┬─────────────┘                           │
  │              ▼                                         │
  │  ┌──────────────────────┐                              │
  │  │ Feature Interaction  │  dot products between        │
  │  │ (dot product layer)  │  all embedding pairs         │
  │  └──────────┬───────────┘                              │
  │              ▼                                         │
  │  ┌──────────────────────┐                              │
  │  │   Top MLP            │                              │
  │  │   → sigmoid → pCTR   │                              │
  │  └─────────────────────┘                              │
  └──────────────────────────────────────────────────────┘
```

### Calibration

```
ML models predict relative ranking well, but probabilities are often wrong.
If model says pCTR = 0.1 but true CTR = 0.05, you'll overcharge advertisers.

Calibration = adjust predicted probabilities to match observed rates.

Platt scaling: sigmoid(a × logit + b), fit a,b on held-out data.
Isotonic regression: non-parametric, bin predictions and map to observed rates.

This is CRITICAL for ad systems because billing depends on accurate pCTR.
```

## 4. Feature Store & Online Serving

```
Challenge: model needs user features at serving time (<10ms).
User's click history, past purchases, etc. must be pre-computed and cached.

┌─────────────────────────────────────────────────────────────────────┐
│                     Feature Pipeline                                 │
│                                                                      │
│  Events (clicks,     ┌──────────┐    ┌────────────┐   ┌──────────┐ │
│  impressions,   ──►  │ Stream   │──► │ Feature    │──►│ Online   │ │
│  purchases)          │ (Kafka/  │    │ Transform  │   │ Store    │ │
│                      │ Flink)   │    │            │   │ (Redis/  │ │
│                      └──────────┘    └────────────┘   │ RocksDB) │ │
│                                                       └─────┬────┘ │
│  Historical    ┌──────────┐    ┌────────────┐               │      │
│  logs     ──►  │ Batch    │──► │ Feature    │──► merge ──────┘      │
│                │ (Spark)  │    │ Compute    │                       │
│                └──────────┘    └────────────┘                       │
│                                                                      │
│  At serving time:                                                    │
│  Ad server ──► feature store ──► [user_features, context_features]  │
│            ──► model server  ──► pCTR prediction                    │
│            total latency: <50ms                                      │
└─────────────────────────────────────────────────────────────────────┘

Near-real-time features (updated in seconds):
  - User's last click (just now)
  - Impressions in current session
  - Real-time trending topics

Batch features (updated hourly/daily):
  - User's 30-day purchase history
  - Ad's historical CTR
  - User segment/cluster membership
```

## 5. Click Aggregation & Attribution

### Click Event Pipeline

```
Billions of events/day. Must handle:
  - Deduplication (user double-clicks)
  - Bot detection (automated clicks, click fraud)
  - Late arrivals (mobile events delayed by hours)
  - Exact-once counting (advertisers pay per click)

Pipeline:
  Click event ──► Kafka ──► Flink/Spark Streaming ──► Aggregation
                                   │
                                   ├── Real-time dashboard (seconds)
                                   ├── Hourly rollups → ClickHouse/Druid
                                   └── Daily billing → PostgreSQL

Scale: Meta processes ~1 trillion ad events per day.
```

### Attribution Models

```
User sees Ad A → sees Ad B → clicks Ad B → buys product.
Who gets credit?

Last-click:     100% credit to Ad B (simplest, most common)
First-click:    100% credit to Ad A
Linear:         50% to A, 50% to B
Time-decay:     More credit to recent touchpoints
Data-driven:    ML model assigns credit (Google's approach)

Conversion window: typically 7 or 30 days after click.
View-through:   user saw ad (didn't click) but later converted.
```

## 6. Ad Targeting

```
How ads find the right users:

┌─────────────────────┬────────────────────────────────────────────────┐
│ Targeting type      │ How it works                                   │
├─────────────────────┼────────────────────────────────────────────────┤
│ Contextual          │ Match ad to page content ("running shoes" ad   │
│                     │ on a fitness article). No user data needed.    │
│ Demographic         │ Age, gender, location, income level.           │
│ Interest-based      │ User's browsing/purchase history → interests.  │
│ Behavioral          │ Retargeting: "user viewed product, show ad."   │
│ Lookalike           │ Find users similar to existing customers.       │
│                     │ Seed audience → embedding → nearest neighbors. │
│ Custom audience     │ Advertiser uploads customer list (email/phone) │
│                     │ Platform matches to user accounts.             │
└─────────────────────┴────────────────────────────────────────────────┘

Post-cookie world (privacy changes):
  - Apple ATT (App Tracking Transparency) killed cross-app tracking
  - Google deprecating 3rd-party cookies in Chrome
  - Solutions: contextual targeting, on-device ML, Privacy Sandbox
  - Meta lost ~$10B/year revenue from ATT changes
```

## 7. Budget Pacing

```
Advertiser sets: "Spend $10,000 today evenly"

Naive approach: bid on everything until budget runs out.
Problem: budget exhausted by 9 AM, miss all afternoon traffic.

Pacing algorithm:
  Target spend rate = $10,000 / 24 hours = $416.67/hour

  If ahead of pace:  lower bid multiplier (bid × 0.8)
  If behind pace:    raise bid multiplier (bid × 1.2)

  ┌──────────────────────────────────────────────┐
  │  Spend ($)                                    │
  │  10K ┤                              ╱ target  │
  │      │                           ╱            │
  │  5K  ┤                        ╱               │
  │      │                     ╱                  │
  │      │                  ╱                     │
  │   0  ┤───────────────────────────────────────│
  │      0    4    8    12   16   20   24  (hour) │
  └──────────────────────────────────────────────┘

  PID controller or online optimization (throttling probability).
  Must also account for traffic patterns (more traffic at 8 PM than 3 AM).
```

## 8. Ad Fraud Detection

```
~$80B/year lost to ad fraud. Types:

Click fraud:     Bots or click farms clicking ads to drain competitor budgets
Impression fraud: Ads loaded but never visible (hidden iframes, pixel-sized)
Install fraud:   Fake app installs to claim CPI payments
Domain spoofing: Fraudulent site pretends to be premium publisher

Detection signals:
  - Click-to-conversion rate anomalies
  - IP/device clustering (1000 clicks from same IP)
  - Click timing patterns (perfectly periodic = bot)
  - Mouse movement analysis (bots don't move mouse naturally)
  - CAPTCHA / proof-of-work challenges
  - ads.txt / sellers.json (publisher verification)
```

## 9. System Architecture — Full Ad Platform

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Full Ad Platform Architecture                         │
│                                                                          │
│  ┌──────────────────────────────────────────────────────────┐           │
│  │ Advertiser-Facing                                         │           │
│  │  Campaign Manager UI → API → Campaign DB (PostgreSQL)     │           │
│  │  Budget, targeting, creatives, bid strategy               │           │
│  └──────────────────────────────────────────────────────────┘           │
│                              │                                           │
│                              ▼                                           │
│  ┌──────────────────────────────────────────────────────────┐           │
│  │ Ad Serving (Hot Path — <100ms)                            │           │
│  │                                                           │           │
│  │  Ad Request → Targeting/Retrieval → Pre-rank → Rank →     │           │
│  │  Auction → Ad Selection → Creative Serving                │           │
│  │                                                           │           │
│  │  Dependencies:                                            │           │
│  │   • Feature Store (Redis) — user features <5ms            │           │
│  │   • Model Server (TF Serving / Triton) — pCTR <20ms      │           │
│  │   • Campaign Index (inverted index) — targeting <5ms      │           │
│  │   • Creative CDN — serve images/video                     │           │
│  └──────────────────────────────────────────────────────────┘           │
│                              │                                           │
│                              ▼                                           │
│  ┌──────────────────────────────────────────────────────────┐           │
│  │ Event Pipeline (Near-Real-Time)                           │           │
│  │                                                           │           │
│  │  Impressions/Clicks/Conversions → Kafka → Flink           │           │
│  │   → Real-time aggregation (ClickHouse)                    │           │
│  │   → Feature updates (Redis)                               │           │
│  │   → Fraud detection                                       │           │
│  │   → Budget tracking / pacing                              │           │
│  └──────────────────────────────────────────────────────────┘           │
│                              │                                           │
│                              ▼                                           │
│  ┌──────────────────────────────────────────────────────────┐           │
│  │ Offline Pipeline (Batch)                                  │           │
│  │                                                           │           │
│  │  Training data generation (Spark) → Model Training (GPU)  │           │
│  │  → Model validation → A/B test → Model deployment         │           │
│  │                                                           │           │
│  │  Daily: attribution, billing reconciliation, reporting    │           │
│  │  Hourly: model retrain, feature recomputation             │           │
│  └──────────────────────────────────────────────────────────┘           │
└─────────────────────────────────────────────────────────────────────────┘
```

## 10. Key Metrics

```
┌──────────────┬─────────────────────────────────────┬───────────────────┐
│ Metric       │ Definition                           │ Typical values    │
├──────────────┼─────────────────────────────────────┼───────────────────┤
│ CTR          │ clicks / impressions                 │ 1-3% (search)     │
│              │                                      │ 0.1% (display)    │
│ CVR          │ conversions / clicks                 │ 2-5%              │
│ CPC          │ cost per click                       │ $0.50 - $5.00     │
│ CPM          │ cost per 1000 impressions            │ $2 - $50          │
│ CPA          │ cost per acquisition/conversion      │ $10 - $200        │
│ ROAS         │ revenue / ad spend                   │ 3-5x is good      │
│ eCPM         │ effective CPM = CTR × CPC × 1000    │ revenue metric    │
│ Fill rate    │ ads served / ad requests             │ 60-90%            │
│ AUC-ROC      │ model discrimination quality         │ 0.75 - 0.85       │
│ Log loss     │ model calibration quality             │ lower is better   │
│ NE           │ normalized entropy (vs baseline)      │ <1.0 is good      │
└──────────────┴─────────────────────────────────────┴───────────────────┘
```

## Numbers to Know

```
Google Ads:     ~10M auctions/sec, $224B revenue (2024)
Meta Ads:       ~100B+ ad impressions/day, $132B revenue (2024)
Amazon Ads:     fastest growing, $47B revenue (2024)
RTB latency:    bid response must be <100ms (often <50ms)
Model serving:  <10ms for pCTR inference
Feature lookup: <5ms from feature store
Training data:  TBs of click logs per day
Model size:     DLRM embedding tables = 1-10 TB
Training freq:  hourly to daily (freshness matters for ads)
```

## Key Papers

| Paper | Year | Contribution |
|-------|------|-------------|
| Ad Click Prediction (Google) | 2013 | FTRL optimizer for sparse LR at scale |
| Wide & Deep (Google) | 2016 | Memorization + generalization |
| DeepFM | 2017 | FM + DNN, no manual cross features |
| DLRM (Meta) | 2019 | Production deep learning rec model |
| DCN v2 (Google) | 2020 | Explicit cross network for interactions |
| DHEN (Meta) | 2022 | Heterogeneous ensemble of interaction modules |

---

## Famous Systems — How They Work Internally

### Google Ads (Search Ads)

```
The world's largest ad platform ($224B revenue, 2024).
Powers Google Search, YouTube, Gmail, Maps, Display Network.

Architecture:

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Google Ads Serving                                │
  │                                                                       │
  │  User searches "running shoes"                                        │
  │       │                                                               │
  │       ▼                                                               │
  │  ┌──────────────────┐                                                │
  │  │ Query Processing  │  Tokenize, spell-correct, extract intent       │
  │  │                   │  Identify commercial intent → trigger ads      │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Ad Retrieval      │  Match query to advertiser keywords            │
  │  │                   │  Match types: exact, phrase, broad             │
  │  │                   │  Broad match uses BERT-based semantic matching │
  │  │                   │  Output: ~10,000 candidate ads                 │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Ad Rank           │  Score = Bid × Quality Score × Extensions      │
  │  │                   │                                                │
  │  │  Quality Score:                                                    │
  │  │   • Expected CTR (ML model, primary signal)                       │
  │  │   • Ad relevance (query-ad text similarity)                       │
  │  │   • Landing page experience (speed, mobile, relevance)            │
  │  │                                                                    │
  │  │  Quality Score 1-10 visible to advertisers                        │
  │  │  (internal score is continuous, much more granular)               │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Auction           │  First-price auction (since 2019)              │
  │  │                   │  Ad Rank threshold: minimum score to show      │
  │  │                   │  Actual CPC ≤ your bid (price to beat next)   │
  │  │                   │  CPC = AdRank(below) / QS(yours) + $0.01     │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Serving           │  Up to 4 ads above organic results             │
  │  │                   │  Up to 3 ads below organic results             │
  │  │                   │  Shopping ads (PLA) in carousel                │
  │  └──────────────────┘                                                │
  │                                                                       │
  │  Key engineering:                                                     │
  │   • Ads index is a massive inverted index (keyword → ads)            │
  │   • Distributed across thousands of machines                         │
  │   • CTR model: evolved from LR → deep learning (billions of params)  │
  │   • Smart Bidding: automated bid strategy using RL                    │
  │   • Broad Match: BERT-based query-keyword semantic matching           │
  │   • Performance Max: multi-channel, fully automated campaigns        │
  └──────────────────────────────────────────────────────────────────────┘

How Quality Score prevents a race to the top:
  Advertiser A: bid $2, Quality Score 8 → Ad Rank = 16 (wins)
  Advertiser B: bid $5, Quality Score 2 → Ad Rank = 10 (loses)
  A pays less AND gets the top spot because their ad is more relevant.
  This aligns Google's incentive (user satisfaction) with revenue.

Google's FTRL (Follow-the-Regularized-Leader) optimizer:
  Problem: billions of sparse features, must train online (streaming data).
  FTRL-Proximal: online learning with L1 regularization.
  L1 → drives most weights to exactly zero → sparse model → memory efficient.
  Can handle trillion-feature models with per-coordinate learning rates.
  This paper (McMahan et al., 2013) defined how large-scale ad models train.
```

### Meta Ads (Facebook / Instagram / WhatsApp)

```
Second largest ad platform ($132B revenue, 2024).
Unique advantage: social graph + first-party data (users logged in).

Architecture:

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Meta Ads Serving                                   │
  │                                                                        │
  │  User opens Facebook/Instagram feed                                    │
  │       │                                                                │
  │       ▼                                                                │
  │  ┌──────────────────┐                                                 │
  │  │ Ad Request        │  User profile, context, ad slot info            │
  │  │                   │  No explicit query (unlike search ads)          │
  │  │                   │  Must PREDICT what user is interested in        │
  │  └──────┬───────────┘                                                 │
  │         ▼                                                              │
  │  ┌──────────────────┐                                                 │
  │  │ Candidate         │  Targeting match (advertiser-defined audiences) │
  │  │ Selection         │  Core Audiences: demographics, interests       │
  │  │                   │  Custom Audiences: uploaded email/phone lists   │
  │  │                   │  Lookalike Audiences: ML-found similar users    │
  │  │                   │  Output: ~10,000 eligible ads                   │
  │  └──────┬───────────┘                                                 │
  │         ▼                                                              │
  │  ┌──────────────────┐                                                 │
  │  │ Ranking           │  Total Value = Bid × Estimated Action Rate     │
  │  │                   │             + User Value                        │
  │  │                   │                                                 │
  │  │  Estimated Action Rate:                                            │
  │  │   • pCTR (click probability)                                       │
  │  │   • pCVR (conversion probability, given click)                     │
  │  │   • P(install), P(purchase), etc.                                  │
  │  │                                                                     │
  │  │  User Value: how much the ad contributes to user experience        │
  │  │   • Negative: low quality, misleading, excessive text in image     │
  │  │   • Positive: relevant, engaging, from a page they follow          │
  │  │                                                                     │
  │  │  Model: DLRM (Deep Learning Recommendation Model)                  │
  │  │   Embedding tables: ~10 TB (every user ID, ad ID, page ID...)     │
  │  │   Trained on: ~1 trillion examples/day                             │
  │  │   Updated: continuously (online learning)                          │
  │  └──────┬───────────┘                                                 │
  │         ▼                                                              │
  │  ┌──────────────────┐                                                 │
  │  │ Auction + Pacing  │  Pacing: spread budget evenly across the day   │
  │  │                   │  Billing: CPC, CPM, or CPA (cost per action)   │
  │  │                   │  Delivery optimization: ML-driven              │
  │  └──────────────────┘                                                 │
  │                                                                        │
  │  Meta's DLRM architecture (production):                                │
  │   • Sparse features: user_id, ad_id, page_id → embedding lookup       │
  │   • Dense features: user_age, ad_historical_ctr → bottom MLP          │
  │   • Feature interaction: dot products of all embedding pairs           │
  │   • Top MLP → sigmoid → pCTR / pCVR                                  │
  │   • Embedding tables = ~95% of model parameters (~10 TB)              │
  │   • Training infrastructure: ZionEX (custom training system)          │
  │   • Serving: runs on custom inference hardware (MTIA chips)           │
  │                                                                        │
  │  Lookalike Audiences:                                                  │
  │   Advertiser uploads seed list (1000 emails of best customers)        │
  │   Meta finds their user profiles → creates embedding centroid          │
  │   ANN search in user embedding space → millions of similar users      │
  │   Expansion: 1% lookalike (most similar) to 10% (broader)            │
  │                                                                        │
  │  Advantage Signal (post-ATT world):                                    │
  │   Apple's ATT killed cross-app tracking (2021).                        │
  │   Meta rebuilt around on-platform signals:                             │
  │   • Conversions API (server-to-server, replaces pixel)                │
  │   • Aggregated Event Measurement (privacy-preserving)                 │
  │   • Advantage+ campaigns: fully automated, minimal targeting          │
  │   • On-device ML for privacy-preserving ad optimization               │
  └──────────────────────────────────────────────────────────────────────┘
```

### Amazon Ads (Retail Media / Sponsored Products)

```
Fastest-growing ad platform ($47B revenue, 2024).
Unique advantage: purchase intent data — users are ALREADY shopping.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Amazon Sponsored Products                         │
  │                                                                       │
  │  User searches "wireless earbuds" on Amazon                           │
  │       │                                                               │
  │       ▼                                                               │
  │  ┌──────────────────┐                                                │
  │  │ Organic Search    │  A9/A10 ranking algorithm                       │
  │  │ + Sponsored       │  Organic: sales velocity, reviews, relevance   │
  │  │                   │  Sponsored: bid × relevance × expected sales   │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Ad Auction        │  Cost-per-click model                          │
  │  │                   │  Key difference from Google/Meta:               │
  │  │                   │  Amazon has PURCHASE data (not just clicks)     │
  │  │                   │  Can directly optimize for ROAS               │
  │  │                   │  (return on ad spend)                          │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Attribution       │  14-day click window                           │
  │  │                   │  Direct purchase attribution                   │
  │  │                   │  Halo effect: ad for product A                 │
  │  │                   │  → user buys product B from same brand         │
  │  └──────────────────┘                                                │
  │                                                                       │
  │  Ad types:                                                            │
  │   Sponsored Products:  appear in search results (biggest revenue)    │
  │   Sponsored Brands:    banner at top of search (brand awareness)     │
  │   Sponsored Display:   retargeting across Amazon + partner sites     │
  │   Amazon DSP:          programmatic display/video (off-Amazon)        │
  │                                                                       │
  │  Why Amazon Ads is different:                                         │
  │   • Purchase intent: user is already shopping (highest intent)       │
  │   • Closed loop: see ad → buy on same platform → perfect attribution │
  │   • First-party data: purchase history, wish lists, reviews          │
  │   • Product catalog data: Amazon knows everything about every product│
  │   • ACoS (advertising cost of sale): ad spend / attributed sales     │
  │     ACoS < profit margin → profitable advertising                    │
  └──────────────────────────────────────────────────────────────────────┘
```

### The Trade Desk (Independent DSP)

```
Largest independent demand-side platform (~$10B market cap).
Buys ads programmatically across all exchanges on behalf of advertisers.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     DSP Architecture (The Trade Desk)                  │
  │                                                                        │
  │  ┌──────────────────┐     ┌──────────────────┐                        │
  │  │ Ad Exchanges      │────►│ Bid Request      │                        │
  │  │ (Google AdX,      │     │ Filter           │ Match to campaigns     │
  │  │  OpenRTB, Index)  │     │ (50ms budget)    │ targeting criteria     │
  │  └──────────────────┘     └──────┬───────────┘                        │
  │                                   ▼                                    │
  │  ┌──────────────────────────────────────────┐                         │
  │  │ Bid Evaluation Pipeline                    │                        │
  │  │                                            │                        │
  │  │  1. User Recognition (UID2 — open-source   │                        │
  │  │     identity framework, replaces cookies)  │                        │
  │  │                                            │                        │
  │  │  2. Campaign matching (which campaigns     │                        │
  │  │     target this user/context?)             │                        │
  │  │                                            │                        │
  │  │  3. Bid calculation:                       │                        │
  │  │     Base bid (from campaign settings)       │                        │
  │  │     × pCVR (ML-predicted conversion rate)   │                        │
  │  │     × bid modifier (time, device, geo)      │                        │
  │  │     × bid shading (first-price auctions)    │                        │
  │  │                                            │                        │
  │  │  4. Bid shading algorithm:                 │                        │
  │  │     First-price auction → bid your value    │                        │
  │  │     = overpay. Shade bid down to estimated  │                        │
  │  │     market clearing price.                  │                        │
  │  │     Uses historical win-price distribution. │                        │
  │  │                                            │                        │
  │  │  5. Submit bid (must respond in <50ms)      │                        │
  │  └──────────────────────────────────────────┘                         │
  │                                                                        │
  │  Scale:                                                                │
  │   • Processes ~15 million bid requests per second                      │
  │   • Evaluates >600 billion ad impressions daily                        │
  │   • Must respond within 50ms per bid request                           │
  │   • Global infrastructure across 10+ data centers                     │
  │                                                                        │
  │  UID2 (Unified ID 2.0):                                                │
  │   Open-source identity framework (post-cookie world)                  │
  │   User's email → deterministic, hashed, encrypted token              │
  │   Allows cross-site targeting without third-party cookies             │
  │   Adopted by: Disney+, Walmart, Target, thousands of publishers      │
  │                                                                        │
  │  Kokai (their AI platform):                                           │
  │   Automated optimization across campaigns                             │
  │   Predictive clearing price (bid shading)                             │
  │   Attention-based measurement (not just viewability)                  │
  │   Cross-device identity graph                                         │
  └──────────────────────────────────────────────────────────────────────┘
```

### Google Ad Exchange (AdX) / Header Bidding

```
How programmatic ads ACTUALLY flow through the ecosystem:

Before header bidding (waterfall, pre-2015):
  Publisher tried ad networks one by one in priority order:
  1. Try Google AdX → if no fill, move on
  2. Try AppNexus → if no fill, move on
  3. Try Rubicon → if no fill, show default ad
  Problem: first network got priority even if others would pay more.

Header bidding (2015+):
  Publisher asks ALL exchanges simultaneously (in browser or server-side):
  ┌───────────────────────────────────────────────────────────────┐
  │  Publisher page loads                                          │
  │       │                                                        │
  │       ▼  (all in parallel)                                     │
  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐         │
  │  │ AdX     │  │ Index   │  │ OpenX   │  │ Pubmatic│         │
  │  │ bid: $3 │  │ bid: $5 │  │ bid: $2 │  │ bid: $4 │         │
  │  └─────────┘  └─────────┘  └─────────┘  └─────────┘         │
  │       │              │            │            │               │
  │       └──────────────┼────────────┼────────────┘               │
  │                      ▼                                         │
  │              Index wins at $5                                  │
  │              (true market price)                               │
  └───────────────────────────────────────────────────────────────┘

  This increased publisher revenue ~20-40% by creating real competition.
  Forced Google AdX to compete fairly → led industry switch to first-price.

Server-side header bidding (Prebid Server):
  Same concept but runs on publisher's server, not in browser.
  Faster (no client-side JS), but publisher loses transparency.
  Amazon TAM (Transparent Ad Marketplace) is a major server-side solution.
```

### TikTok Ads (ByteDance)

```
Fastest-growing ad platform. Unique: video-first, creator economy.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     TikTok Ads Architecture                           │
  │                                                                       │
  │  Ad formats:                                                          │
  │   In-Feed ads:      appear in For You Page (look like organic content)│
  │   TopView:          first video when app opens (premium)             │
  │   Branded Hashtag:  sponsor a hashtag challenge                      │
  │   Spark Ads:        boost existing organic creator content as ad     │
  │                                                                       │
  │  What's different:                                                    │
  │                                                                       │
  │  1. Creative IS the targeting                                        │
  │     Traditional: target audience → show ad                            │
  │     TikTok: upload creative → ML finds the right audience             │
  │     The algorithm figures out who resonates with the video            │
  │     Advertisers often say: "broad targeting + great creative wins"   │
  │                                                                       │
  │  2. Content understanding                                             │
  │     Video → multimodal embedding (visual + audio + text)              │
  │     This embedding IS the targeting signal                            │
  │     "Show this cooking ad to people who watch cooking content"       │
  │     No need for explicit interest targeting                           │
  │                                                                       │
  │  3. Creator-advertiser integration                                    │
  │     Spark Ads: advertiser sponsors a creator's organic post           │
  │     → same engagement metrics, same viral potential                   │
  │     → blurs line between content and advertising                     │
  │     → higher engagement than traditional ads (~2x CTR)               │
  │                                                                       │
  │  Ranking model:                                                       │
  │   Similar multi-stage funnel as Meta                                  │
  │   Heavy use of video embeddings for matching                          │
  │   Multi-objective: P(click), P(watch_6s), P(engagement)             │
  │   pCVR estimated differently: many conversions happen OFF-platform   │
  │   → TikTok Pixel + Events API for conversion tracking                │
  │                                                                       │
  │  Smart Performance Campaigns:                                         │
  │   Fully automated (like Meta's Advantage+)                           │
  │   Input: creative + landing page + budget + target CPA               │
  │   ML handles: audience, placement, bidding, creative selection       │
  └──────────────────────────────────────────────────────────────────────┘
```

### Criteo (Retargeting / Commerce Media)

```
Specialist in retargeting ads (you viewed shoes → see shoe ads everywhere).

How retargeting works internally:

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Retargeting Pipeline                              │
  │                                                                       │
  │  1. User visits advertiser site, views product                        │
  │     Criteo pixel fires → event logged                                │
  │     User cookie/device ID → product viewed → stored                  │
  │                                                                       │
  │  2. User visits publisher site (CNN, weather.com)                     │
  │     Publisher's ad slot → bid request to Criteo                       │
  │     Criteo recognizes user → retrieves browsing history               │
  │                                                                       │
  │  3. Product recommendation engine:                                    │
  │     Not just "show the product they viewed"                           │
  │     ML model predicts best product to show from catalog:             │
  │     • The viewed product (if recently viewed, high intent)           │
  │     • Similar products (same category, different brand)              │
  │     • Complementary products (viewed shoes → show socks)             │
  │     • Personalized selection from full catalog                        │
  │                                                                       │
  │  4. Dynamic Creative Optimization (DCO):                              │
  │     Generate ad creative in real-time:                                │
  │     Product image + price + "20% off" banner → rendered on the fly   │
  │     A/B test layouts, colors, copy automatically                     │
  │     Personalized per user (show their viewed products)               │
  │                                                                       │
  │  5. Bid + serve in <100ms                                             │
  │                                                                       │
  │  Criteo's ML engine:                                                  │
  │   • Predictive bidding: P(click) × P(purchase|click) × expected value│
  │   • Handles ~4B bid requests/day                                      │
  │   • Product-level embeddings for catalog recommendation              │
  │   • Online learning: model updates every few minutes                 │
  └──────────────────────────────────────────────────────────────────────┘
```
