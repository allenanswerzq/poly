# Recommendation Systems Deep Dive

## Overview

Recommendation systems predict what a user will like based on their history, similar users' behavior, and item attributes. They power the feeds of Netflix, YouTube, TikTok, Amazon, Spotify, and virtually every consumer app. The core challenge: from millions of items, find the ~10 most relevant for THIS user RIGHT NOW.

## The Big Picture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                  Recommendation: End-to-End Flow                        │
│                                                                         │
│   User opens       Context           Multi-stage         Feed/          │
│   the app    ──►  (who, when,  ──►  Retrieval +   ──►  Recommendations │
│                    where, device)    Ranking              shown          │
│                                     (<200ms)                            │
│                                                                         │
│   User watches/    Implicit          Offline              Models        │
│   clicks/skips ──► signals     ──►  Training      ──►   Updated        │
│                    logged            (daily/hourly)                      │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘

The feedback loop is EVERYTHING:
  Better model → better recommendations → more engagement →
  more training data → better model → ...
```

## 1. Types of Recommendation

```
┌──────────────────┬──────────────────────────────┬───────────────────────┐
│ Type             │ How it works                  │ Example               │
├──────────────────┼──────────────────────────────┼───────────────────────┤
│ Collaborative    │ "Users like you also liked X" │ Netflix, Spotify      │
│ filtering (CF)   │ Based on user behavior only   │                       │
│                  │                                │                       │
│ Content-based    │ "You liked action movies,     │ Pandora (Music Genome)│
│                  │  here's another action movie"  │                       │
│                  │ Based on item attributes       │                       │
│                  │                                │                       │
│ Hybrid           │ Combine CF + content + context │ YouTube, TikTok      │
│                  │ Deep learning models           │                       │
│                  │                                │                       │
│ Knowledge-based  │ "You said budget < $500,      │ Real estate,         │
│                  │  here are matching laptops"    │ travel search         │
│                  │ Based on explicit requirements │                       │
└──────────────────┴──────────────────────────────┴───────────────────────┘
```

## 2. Collaborative Filtering — The Foundation

### User-Based CF

```
Find users similar to you → recommend what they liked.

User-Item interaction matrix (ratings 1-5, 0 = not rated):

              Movie A  Movie B  Movie C  Movie D  Movie E
  Alice:        5        3        4        ?        1
  Bob:          4        ?        5        3        2
  Carol:        ?        2        4        5        ?
  Dave:         5        3        ?        4        1

Alice hasn't rated Movie D. Who is most similar to Alice?
  sim(Alice, Bob)  = cosine(Alice_vec, Bob_vec) = 0.92  ← very similar
  sim(Alice, Dave) = cosine(Alice_vec, Dave_vec) = 0.98  ← most similar

Dave rated Movie D = 4 → predict Alice will rate Movie D ≈ 4.

Problem: O(N²) to compute user similarities. Doesn't scale to millions of users.
```

### Item-Based CF (Amazon, 2003)

```
"Customers who bought X also bought Y"

Instead of user similarity, compute ITEM similarity:
  sim(Movie A, Movie D) = cosine of their rating columns

  Movie A ratings: [5, 4, ?, 5]
  Movie D ratings: [?, 3, 5, 4]

Item similarities are more stable than user similarities
(items don't change, users' tastes drift).

Pre-compute item-item similarity matrix (offline).
At serving time: look up items user liked → find similar items → rank.
O(K) per recommendation where K = items user has interacted with.

This is what Amazon uses for "Customers who bought this also bought..."
```

### Matrix Factorization (Netflix Prize, 2006-2009)

```
The breakthrough: decompose the user-item matrix into latent factors.

  R ≈ U × V^T

  R: user×item rating matrix (mostly sparse/unknown)
  U: user×k matrix (k latent factors per user, k ≈ 50-200)
  V: item×k matrix (k latent factors per item)

  Each user = a k-dimensional vector (embedding)
  Each item = a k-dimensional vector (embedding)
  Predicted rating = dot product of user and item embeddings

  ┌─────────────────────────────────────────────────────────┐
  │  User "Alice" embedding: [0.8, -0.3, 0.5, ..., 0.1]   │
  │  Movie "Inception" embedding: [0.7, -0.2, 0.6, ..., 0.0]│
  │                                                          │
  │  predicted_rating = dot(Alice, Inception) = 0.56 + ...   │
  │                   = 4.2 out of 5                         │
  │                                                          │
  │  The latent factors might represent (learned, not labeled):│
  │    factor 0: action vs romance                           │
  │    factor 1: mainstream vs indie                         │
  │    factor 2: recent vs classic                           │
  │    ...                                                    │
  └─────────────────────────────────────────────────────────┘

Training: minimize ||R - U×V^T||² + λ(||U||² + ||V||²)
  over observed ratings only (don't train on unknowns).
  Optimized with ALS (Alternating Least Squares) or SGD.

This won the Netflix Prize ($1M) and is still used today.
```

## 3. Deep Learning Recommenders

### Two-Tower Model (Google, YouTube, 2019)

```
The workhorse of modern recommendations at scale.
Same idea as bi-encoder in search.

  ┌──────────────────────────────────────────────────────────────┐
  │  Two-Tower Architecture                                       │
  │                                                               │
  │  User features              Item features                     │
  │  (age, history,             (title, category,                │
  │   device, time)              popularity, tags)                │
  │       │                           │                           │
  │       ▼                           ▼                           │
  │  ┌──────────┐              ┌──────────┐                       │
  │  │ User     │              │ Item     │                       │
  │  │ Tower    │              │ Tower    │                       │
  │  │ (DNN)    │              │ (DNN)    │                       │
  │  └────┬─────┘              └────┬─────┘                       │
  │       │                         │                             │
  │       ▼                         ▼                             │
  │  user_embedding           item_embedding                      │
  │  (64-256 dims)            (64-256 dims)                      │
  │       │                         │                             │
  │       └──────────┬──────────────┘                             │
  │                  ▼                                             │
  │            dot product → score                                │
  │                                                               │
  │  Key insight:                                                  │
  │   Item embeddings computed OFFLINE → stored in ANN index      │
  │   User embedding computed ONLINE (~5ms)                       │
  │   ANN search for nearest items (~10ms over millions of items) │
  └──────────────────────────────────────────────────────────────┘

  Why two towers?
    Can't score every item with a heavy model (millions of items).
    Pre-compute item embeddings → ANN retrieval in milliseconds.
    Same approach as dense retrieval in search.
```

### Deep & Cross / Feature Interaction Models

```
For the RANKING stage (not retrieval), we need richer models:

DCN v2 (Google, 2020):
  Cross network explicitly learns feature interactions:
    x_{l+1} = x_0 ⊙ (W_l · x_l + b_l) + x_l

  "Users aged 25-35 who watched sci-fi on weekends" — that's a 3rd-order
  interaction (age × genre × time). Cross network learns these automatically.

DIN — Deep Interest Network (Alibaba, 2018):
  User's click history is a SEQUENCE of items.
  Not all past items are equally relevant to the current candidate.
  Attention mechanism: weight past items by relevance to candidate.

  "User clicked [shoes, laptop, shoes, shirt, shoes]"
  Candidate: running shoes
  → attention upweights past shoe clicks, downweights laptop/shirt.

  ┌──────────────────────────────────────────────┐
  │  DIN: Attention over user history             │
  │                                               │
  │  History: [shoe₁, laptop, shoe₂, shirt, shoe₃]│
  │  Candidate: running_shoe                      │
  │                                               │
  │  Attention weights:                           │
  │    shoe₁:  0.35  ← relevant                  │
  │    laptop: 0.02  ← irrelevant                │
  │    shoe₂:  0.30  ← relevant                  │
  │    shirt:  0.03  ← irrelevant                │
  │    shoe₃:  0.30  ← relevant                  │
  │                                               │
  │  Weighted sum → user interest representation  │
  │  Concat with other features → MLP → score     │
  └──────────────────────────────────────────────┘

SIM — Search-Based Interest Model (Alibaba, 2020):
  DIN is O(history_length) → expensive for users with 10K+ interactions.
  SIM: first retrieve relevant history items (search), then attend.
  Two-stage: General Search Unit (fast filter) → Exact Search Unit (precise).
```

### Sequence Models (Transformers for Recommendations)

```
Treat user's interaction history as a SEQUENCE (like a sentence in NLP).
Predict the next item the user will interact with.

SASRec — Self-Attentive Sequential Recommendation (2018):
  User history: [item₁, item₂, item₃, item₄, ?]
  Model: Transformer (self-attention) over item embeddings.
  Predict: item₅ = softmax over all items.

  Like a language model, but for items instead of words.

BERT4Rec (2019):
  Masked item prediction (like BERT's MLM).
  Randomly mask items in history → predict them.
  Bidirectional attention (sees both past and future context).

  History: [shoes, [MASK], laptop, shirt, [MASK]]
  Predict: what goes in the [MASK] positions?

These work well for session-based recommendations
(e.g., what to suggest next on an e-commerce browsing session).
```

## 4. Multi-Stage Recommendation Pipeline

```
Same funnel pattern as ads and search:

┌──────────────────────────────────────────────────────────────────┐
│  Stage 1: CANDIDATE GENERATION (Retrieval)                       │
│  Pool: millions of items                                         │
│  Methods:                                                        │
│   • Two-tower model + ANN index                                  │
│   • Item-based CF ("similar to your recent views")               │
│   • Popular/trending items                                       │
│   • Social graph ("your friends liked")                          │
│   • Rule-based (new items, geographic, editorial picks)          │
│  Output: ~1,000 candidates from multiple sources                 │
│  Latency: ~20ms                                                  │
├──────────────────────────────────────────────────────────────────┤
│  Stage 2: PRE-RANKING (lightweight scoring)                      │
│  Small model (distilled from heavy ranker)                       │
│  Quick score to cut candidates                                   │
│  Output: ~200 candidates                                         │
│  Latency: ~10ms                                                  │
├──────────────────────────────────────────────────────────────────┤
│  Stage 3: RANKING (heavy model)                                  │
│  DCN v2, DIN, or similar deep model                              │
│  Rich features: user profile, item features, context,            │
│  cross features, sequence features                               │
│  Multi-objective: P(click), P(like), P(share), P(purchase)       │
│  Output: scored and ranked candidates                            │
│  Latency: ~50ms                                                  │
├──────────────────────────────────────────────────────────────────┤
│  Stage 4: RE-RANKING + BUSINESS LOGIC                            │
│  Diversity: don't show 5 cat videos in a row                     │
│  Freshness: boost new content                                    │
│  Explore vs exploit: inject some uncertain items                 │
│  Business rules: promote own content, contractual obligations    │
│  Filter: remove already-seen, policy violations                  │
│  Output: final feed (10-50 items per page load)                  │
│  Latency: ~10ms                                                  │
└──────────────────────────────────────────────────────────────────┘
```

## 5. Multi-Objective Optimization

```
Users can: click, like, share, comment, watch 50%, watch 100%, purchase, hide...

Each action has a different value:
  click:        value = 1
  like:         value = 2
  share:        value = 5
  purchase:     value = 10
  hide/report:  value = -10  (negative!)

Final score = weighted combination of predicted probabilities:

  score = w₁ × P(click) + w₂ × P(like) + w₃ × P(share)
        + w₄ × P(purchase) - w₅ × P(hide)

  ┌─────────────────────────────────────────────────────┐
  │ Multi-task model architecture:                       │
  │                                                      │
  │  Shared features → Shared bottom layers              │
  │                         │                            │
  │               ┌─────────┼─────────┐                  │
  │               ▼         ▼         ▼                  │
  │          ┌────────┐ ┌────────┐ ┌────────┐            │
  │          │ Click  │ │ Like   │ │Purchase│            │
  │          │ tower  │ │ tower  │ │ tower  │            │
  │          └───┬────┘ └───┬────┘ └───┬────┘            │
  │              │          │          │                  │
  │          P(click)   P(like)   P(purchase)            │
  │                                                      │
  │  Shared-bottom vs MMOE (Mixture of Experts):         │
  │    MMOE has multiple expert networks + gating         │
  │    network that selects experts per task.             │
  │    Better handles conflicting objectives.            │
  └─────────────────────────────────────────────────────┘

YouTube's objective (simplified):
  score = P(click) × expected_watch_time
  They optimize for WATCH TIME, not just clicks.
  This prevents clickbait (high click, low watch time → low score).
```

## 6. Cold Start Problem

```
New user: no interaction history → can't do collaborative filtering.
New item: no one has interacted with it → invisible to CF models.

┌────────────────────┬────────────────────────────────────────────────────┐
│ Problem            │ Solutions                                          │
├────────────────────┼────────────────────────────────────────────────────┤
│ New user           │ • Show popular/trending items (popularity bias)   │
│                    │ • Ask for preferences during onboarding           │
│                    │ • Use demographics/device/location for initial recs│
│                    │ • Contextual bandits (explore to learn fast)      │
│                    │ • Transfer learning from other platforms           │
│                    │                                                    │
│ New item           │ • Content-based features (title, category, image) │
│                    │ • Inject into explore traffic (small % of users)  │
│                    │ • Use item metadata + embedding from similar items│
│                    │ • Editor/curator picks for initial exposure        │
│                    │                                                    │
│ New system         │ • Start with content-based or popularity          │
│                    │ • Migrate to CF once you have interaction data    │
│                    │ • Use transfer learning from pre-trained models   │
└────────────────────┴────────────────────────────────────────────────────┘
```

## 7. Explore vs Exploit (Bandits)

```
Exploit: show items the model is confident user will like.
Explore: show items the model is UNCERTAIN about (to learn).

Pure exploit → filter bubble, miss new interests, new items never shown.
Pure explore → bad user experience (random recommendations).

┌─────────────────────┬──────────────────────────────────────────────┐
│ Strategy            │ How it works                                 │
├─────────────────────┼──────────────────────────────────────────────┤
│ ε-greedy            │ 95% exploit (best), 5% random explore       │
│ Thompson Sampling   │ Sample from posterior of predicted quality   │
│                     │ Uncertain items get explored naturally       │
│ UCB (Upper Conf.    │ score = predicted + confidence_bonus         │
│  Bound)             │ New items have high uncertainty → explored   │
│ Contextual Bandits  │ Bandit that conditions on user/context       │
│                     │ Used at Netflix, Spotify for exploration     │
└─────────────────────┴──────────────────────────────────────────────┘

TikTok's approach:
  New video → show to small random audience → measure engagement
  → if high engagement, expand to larger audience → repeat
  This is effectively a bandit that explores new content aggressively.
```

## 8. Embedding-Based Retrieval

```
The foundation of modern recommendation retrieval.

Learn embeddings such that:
  user_embedding · item_embedding → high if user likes item

Training data from implicit feedback:
  Positive pairs: (user, item they clicked/watched/bought)
  Negative pairs: (user, random item)  ← careful negative sampling matters!

  Loss function (simplified):
    maximize: dot(user, positive_item)
    minimize: dot(user, negative_item)
    Using: contrastive loss, triplet loss, or sampled softmax

Negative sampling strategies:
  Random:       easy negatives, model learns fast but plateaus
  Hard negatives: items that are similar but user didn't engage with
                  Model learns finer distinctions. Much better quality.
  In-batch:     use other users' positive items as negatives (efficient)

  ┌──────────────────────────────────────────────────────────────┐
  │ User embeddings          Item embeddings                      │
  │                                                               │
  │     u₁ ●                    ● i₃ (romance)                   │
  │                                                               │
  │   u₂ ●                ● i₁ (action)                          │
  │                        ● i₂ (action)                          │
  │                                                               │
  │        u₃ ●                                                   │
  │                                    ● i₄ (documentary)        │
  │                                                               │
  │  u₂ is close to i₁, i₂ → recommend action movies to u₂      │
  │  u₃ is close to i₄ → recommend documentaries to u₃           │
  └──────────────────────────────────────────────────────────────┘

  At serving time:
    1. Compute user_embedding (online, ~5ms)
    2. ANN search over item_embeddings (pre-indexed, ~10ms)
    3. Return top-K nearest items
```

## 9. Feature Engineering for Recommendations

```
┌─────────────────┬────────────────────────────────────────────────────┐
│ Category        │ Features                                           │
├─────────────────┼────────────────────────────────────────────────────┤
│ User profile    │ age, gender, country, signup_date, device          │
│ User behavior   │ click_count_7d, watch_hours_30d, genres_watched,  │
│ (aggregated)    │ avg_session_length, purchase_frequency             │
│ User sequence   │ last_N_items_viewed (order matters)                │
│ Item attributes │ category, tags, price, creator, duration, quality │
│ Item stats      │ global_ctr, total_views, avg_rating, freshness    │
│ Context         │ time_of_day, day_of_week, device, location        │
│ Cross features  │ user_genre_affinity, user_price_range,            │
│                 │ user×item_category interaction                     │
│ Social          │ friends_who_liked, social_proof_count              │
│ Real-time       │ items_in_current_session, time_since_last_click   │
└─────────────────┴────────────────────────────────────────────────────┘
```

## 10. Evaluation & Metrics

### Offline Metrics

```
┌──────────────────┬───────────────────────────────────────────────────┐
│ Metric           │ What it measures                                  │
├──────────────────┼───────────────────────────────────────────────────┤
│ Precision@K      │ Of top K recs, how many did user interact with?  │
│ Recall@K         │ Of all relevant items, how many in top K?        │
│ NDCG@K           │ Ranking quality (graded relevance)               │
│ Hit Rate@K       │ Does at least one relevant item appear in top K? │
│ MRR              │ 1/rank of first relevant item                    │
│ AUC-ROC          │ Model discrimination (positive vs negative)      │
│ Coverage         │ What % of items ever get recommended?            │
│ Diversity        │ How different are items in a recommendation set? │
│ Novelty          │ Are we recommending non-obvious items?            │
│ Serendipity      │ Are recs surprising AND relevant?                │
└──────────────────┴───────────────────────────────────────────────────┘
```

### Online Metrics (A/B Tests)

```
Offline metrics don't tell the full story. Must A/B test.

┌─────────────────────┬──────────────────────────────────────────┐
│ Engagement metrics  │ CTR, watch time, likes, shares, comments │
│ Retention metrics   │ DAU, WAU, churn rate, session frequency  │
│ Business metrics    │ Revenue, conversions, GMV, subscriptions │
│ Quality metrics     │ User satisfaction surveys, NPS           │
│ Ecosystem health    │ Creator uploads, content diversity       │
└─────────────────────┴──────────────────────────────────────────┘

Proxy metrics vs true metrics:
  Optimizing for clicks → clickbait
  Optimizing for watch time → addictive content
  Optimizing for long-term retention → better for users AND business
  This is why YouTube shifted from clicks to "satisfaction" signals.
```

## 11. Real-World Systems — Deep Dives

### YouTube Recommendations (Google)

```
The largest video recommendation system in the world.
~500 hours of video uploaded per minute. >1 billion hours watched/day.
~70% of watch time comes from recommendations.

Architecture (based on 2016 paper + subsequent evolution):

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     YouTube RecSys Architecture                       │
  │                                                                       │
  │  Stage 1: CANDIDATE GENERATION (~1000 candidates)                    │
  │                                                                       │
  │   Multiple retrieval sources, merged:                                │
  │                                                                       │
  │   a) Deep Neural Network retriever:                                  │
  │      User history (last 50 watch IDs) → embedding layer              │
  │      + context (time, device, geography)                              │
  │      → dense layers → user embedding (256-dim)                       │
  │      → ANN search over all video embeddings                          │
  │      Key trick: training as extreme multiclass classification         │
  │      (softmax over millions of videos, sampled softmax in practice)  │
  │                                                                       │
  │   b) Collaborative filtering:                                        │
  │      "Users who watched X also watched Y"                            │
  │      Co-watch graph → random walks → candidate videos                │
  │                                                                       │
  │   c) Subscriptions and following:                                    │
  │      New uploads from subscribed channels                            │
  │                                                                       │
  │   d) Trending / seed videos:                                         │
  │      Currently popular in user's region                              │
  │                                                                       │
  │  Stage 2: RANKING (score ~1000 candidates → top ~30)                 │
  │                                                                       │
  │   Deep neural network with rich features:                            │
  │                                                                       │
  │   Features:                                                           │
  │    • Video embeddings (visual + audio + text/title)                  │
  │    • User's watch/search history embeddings                          │
  │    • Freshness (video age — YouTube HEAVILY boosts fresh content)    │
  │    • User's language, location, device                               │
  │    • Previous impressions of this video (has user seen it before?)   │
  │    • Time since last watch (longer → lower score)                    │
  │    • Channel features (subscriber count, upload frequency)           │
  │                                                                       │
  │   THE KEY INSIGHT: optimize for WATCH TIME, not clicks.              │
  │                                                                       │
  │   score = P(click) × E[watch_time | click]                          │
  │                                                                       │
  │   Implementation: weighted logistic regression.                      │
  │     Positive examples weighted by watch duration:                    │
  │     - Watched 60 seconds → weight = 60                               │
  │     - Clicked but left in 2 sec → weight = 2                         │
  │     - Didn't click → weight = 0 (negative example)                  │
  │                                                                       │
  │   At inference: model outputs odds, which approximate               │
  │   E[watch_time]. Videos with high expected watch time rank higher.   │
  │                                                                       │
  │   This single insight eliminated most clickbait:                     │
  │     Clickbait: high P(click), low watch_time → low score.           │
  │     Quality content: moderate P(click), high watch_time → high score.│
  │                                                                       │
  │  Stage 3: RE-RANKING + POLICIES                                      │
  │                                                                       │
  │   • Diversity: spread topics/creators, avoid repetition              │
  │   • Authoritativeness: boost reliable sources for medical/news       │
  │   • Responsible AI: reduce borderline content                        │
  │   • Ad slot insertion (interleave ads at natural break points)       │
  │                                                                       │
  │  Post-2020 evolution:                                                 │
  │   • Shifted from clicks/watch-time to "satisfaction" signals          │
  │   • Added: likes, "not interested", surveys ("was this valuable?")   │
  │   • Multi-objective: engagement + satisfaction + diversity            │
  │   • Short-form (Shorts): separate recommendation system              │
  │     Shorts model: more like TikTok (swipe-based, colder start)      │
  └──────────────────────────────────────────────────────────────────────┘

Infrastructure:
  • Candidate generation models run on Google's TPU pods
  • Ranking model: large transformer with ~100B+ parameters
  • Feature store: Google's internal Spanner + Bigtable
  • Model updated continuously (not daily batches)
  • Serving latency: <200ms end-to-end
```

### TikTok / Douyin (ByteDance)

```
The most talked-about recommendation system in the industry.
~1.5 billion monthly active users. Average session: 52 minutes.
Entirely recommendation-driven (no explicit "follow" feed by default).

What makes TikTok's RecSys exceptional:

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     TikTok RecSys Architecture                        │
  │                                                                       │
  │  1. COLD START (the signature advantage)                              │
  │                                                                       │
  │     New user → show ~8 diverse videos from different categories      │
  │     Measure: watch time per video (not clicks — there ARE no clicks)  │
  │                                                                       │
  │     After 8 videos, the system has:                                   │
  │      - Which categories held attention                               │
  │      - What video length is preferred                                │
  │      - Audio vs visual preference (music vs. talking)                │
  │      - Humor vs educational vs dramatic                              │
  │                                                                       │
  │     After ~30 videos: reasonably personalized feed                    │
  │     After ~100 videos: highly personalized                            │
  │                                                                       │
  │     Why it works: short videos (15-60s) = dense signal per minute.   │
  │     YouTube: user watches 1 video in 10 min → 1 signal.             │
  │     TikTok: user watches 10 videos in 10 min → 10 signals.          │
  │                                                                       │
  │  2. SIGNALS (ranked by importance)                                    │
  │                                                                       │
  │     ┌─────────────────────────────────────────────────┐              │
  │     │ Signal              │ Strength   │ Why            │              │
  │     ├─────────────────────┼────────────┼────────────────┤              │
  │     │ Completion rate     │ ★★★★★     │ Watched till   │              │
  │     │ (% of video watched)│            │ end = loved it │              │
  │     │ Replay             │ ★★★★★     │ Watched AGAIN  │              │
  │     │ Share              │ ★★★★☆     │ Worth sharing  │              │
  │     │ Comment            │ ★★★☆☆     │ Engaged enough │              │
  │     │                    │            │ to type        │              │
  │     │ Like               │ ★★★☆☆     │ Positive but   │              │
  │     │                    │            │ low effort     │              │
  │     │ Follow (from video)│ ★★★★☆     │ Strong intent  │              │
  │     │ "Not interested"   │ ★★★★★     │ Explicit neg   │              │
  │     │ Skip (<2s)         │ ★★☆☆☆     │ Weak negative  │              │
  │     │ (scroll past)      │            │ (maybe just    │              │
  │     │                    │            │ not in mood)   │              │
  │     └─────────────────────┴────────────┴────────────────┘              │
  │                                                                       │
  │  3. CONTENT UNDERSTANDING (multimodal)                                │
  │                                                                       │
  │     Video  → visual embedding (what's in the video)                  │
  │     Audio  → audio embedding (music, speech, effects)                │
  │     Text   → NLP on captions, hashtags, overlaid text               │
  │     OCR    → text extracted from video frames                        │
  │     ASR    → speech-to-text transcript                               │
  │                                                                       │
  │     All embeddings concatenated → video content vector               │
  │     This allows TikTok to understand NEW videos immediately          │
  │     (content-based, no need for engagement history)                  │
  │                                                                       │
  │  4. ARCHITECTURE                                                      │
  │                                                                       │
  │     Retrieval: multiple sources                                       │
  │      • Content-based: similar to recently watched (embedding ANN)    │
  │      • Collaborative: users with similar patterns liked this         │
  │      • Social: friends/followed creators' content                    │
  │      • Trending: globally/locally popular                            │
  │      • Explore: random injection for new categories                  │
  │                                                                       │
  │     Ranking: deep model                                               │
  │      • Multi-objective: P(completion) × w₁ + P(like) × w₂           │
  │        + P(share) × w₃ + P(comment) × w₄ - P(skip) × w₅            │
  │      • Real-time features: current session behavior                  │
  │      • User interest decay: recent interests weighted more           │
  │                                                                       │
  │     Re-ranking:                                                       │
  │      • Creator fairness: spread exposure across creators             │
  │      • Topic diversity: don't show 5 dance videos in a row           │
  │      • Temporal diversity: mix old and new content                   │
  │      • Brand safety filters                                          │
  │                                                                       │
  │  5. THE INTEREST GRAPH (vs Social Graph)                              │
  │                                                                       │
  │     Facebook/Instagram: recommendations based on social connections  │
  │     TikTok: recommendations based on INTERESTS (no friends needed)   │
  │                                                                       │
  │     This is why TikTok works for new users:                          │
  │      • Don't need friends on the platform                            │
  │      • Don't need to follow anyone                                   │
  │      • Content finds YOU based on behavior                           │
  │      • Creates "interest communities" that cross social boundaries   │
  └──────────────────────────────────────────────────────────────────────┘
```

### Netflix (Personalization Everywhere)

```
~260 million subscribers. ~80% of content watched is recommended.
Netflix doesn't just recommend WHAT to watch — it personalizes HOW you see it.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Netflix RecSys Architecture                       │
  │                                                                       │
  │  1. HOMEPAGE PERSONALIZATION                                          │
  │                                                                       │
  │     The homepage is a grid: rows × columns.                          │
  │     EVERYTHING is personalized:                                       │
  │                                                                       │
  │     ┌───────────────────────────────────────────────────┐            │
  │     │ Top 10 in Your Country            ← row selection │            │
  │     │ [Movie A] [Movie B] [Movie C]     ← item order   │            │
  │     ├───────────────────────────────────────────────────┤            │
  │     │ Because You Watched "Dark"        ← row title     │            │
  │     │ [Show D] [Show E] [Show F]        ← item selection│            │
  │     ├───────────────────────────────────────────────────┤            │
  │     │ Critically Acclaimed Thrillers     ← row type     │            │
  │     │ [Movie G] [Movie H] [Movie I]                     │            │
  │     └───────────────────────────────────────────────────┘            │
  │                                                                       │
  │     Three ML problems:                                                │
  │      a) Row selection: which rows to show (from ~10K candidate rows) │
  │      b) Row ordering: which rows go on top                           │
  │      c) Within-row ranking: which titles go first (leftmost = best) │
  │                                                                       │
  │     Modeled as a two-level optimization:                              │
  │      Page-level: maximize P(user finds something to watch)           │
  │      Row-level: maximize P(user engages with this row)               │
  │                                                                       │
  │  2. ARTWORK PERSONALIZATION                                           │
  │                                                                       │
  │     The same movie shows DIFFERENT thumbnail images to different users│
  │                                                                       │
  │     User A (likes romance):                                           │
  │       "Pulp Fiction" → thumbnail showing Uma Thurman                  │
  │     User B (likes action):                                            │
  │       "Pulp Fiction" → thumbnail showing John Travolta with gun      │
  │                                                                       │
  │     Netflix pre-generates ~20 thumbnails per title.                   │
  │     Contextual bandit selects which thumbnail to show each user.     │
  │     Increased engagement significantly (published in 2016 blog).     │
  │                                                                       │
  │  3. RECOMMENDATION ALGORITHMS                                         │
  │                                                                       │
  │     a) Personalized Video Ranker (PVR):                               │
  │        General-purpose ranker. Features: viewing history, ratings,   │
  │        metadata (genre, actors, director), popularity, freshness.    │
  │        Trained with pairwise learning to rank.                       │
  │                                                                       │
  │     b) Because You Watched (BYW):                                    │
  │        For each recently watched title, find similar titles.         │
  │        Item-item similarity using co-viewing patterns + metadata.    │
  │        "You watched Stranger Things → Dark, The OA, Black Mirror"   │
  │                                                                       │
  │     c) Top-N Video Ranker:                                           │
  │        Rank entire catalog for each user (offline, batch).           │
  │        Matrix factorization + neural collaborative filtering.        │
  │        Pre-computed, cached, refreshed daily.                        │
  │                                                                       │
  │     d) Trending Now:                                                  │
  │        Time-sensitive popularity. Boosted by:                        │
  │        - Absolute popularity (# views)                               │
  │        - Velocity (rate of increase in views)                        │
  │        - Personalized trending (trending in YOUR taste cluster)      │
  │                                                                       │
  │     e) Continue Watching:                                             │
  │        Rank partially-watched titles. Consider:                      │
  │        - Recency of last watch                                       │
  │        - % completed (80% done → less likely to return)              │
  │        - Time of day patterns (user watches different shows AM vs PM)│
  │                                                                       │
  │  4. THE TASTE COMMUNITIES APPROACH                                    │
  │                                                                       │
  │     Netflix clusters ~260M users into ~2000 "taste communities"      │
  │     Not by demographics — by viewing behavior.                       │
  │     "K-drama fans who also like true crime documentaries"            │
  │     Each community has a distinct preference profile.                │
  │     New or cold-start users assigned to communities quickly.         │
  │                                                                       │
  │  5. A/B TESTING AT SCALE                                              │
  │                                                                       │
  │     Netflix runs ~250 A/B tests simultaneously.                      │
  │     Every recommendation change is A/B tested.                       │
  │     Primary metric: member retention (not short-term engagement).    │
  │     Secondary: hours watched, diversity of consumption.              │
  │     They built their own experimentation platform (XP).              │
  │                                                                       │
  │  Infrastructure:                                                      │
  │   • All on AWS (one of AWS's biggest customers)                      │
  │   • Apache Spark for batch training                                  │
  │   • Cassandra for real-time feature store                            │
  │   • Custom serving layer (Zuul gateway + microservices)              │
  │   • Personalization happens at CDN edge for speed                    │
  └──────────────────────────────────────────────────────────────────────┘
```

### Spotify (Music & Podcast Recommendations)

```
~600 million users, ~100 million songs, ~5 million podcasts.
Music recommendations have unique challenges vs video/e-commerce.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Spotify RecSys Architecture                       │
  │                                                                       │
  │  Unique challenges of music:                                          │
  │   • Re-consumption: users replay favorites (unlike movies/shopping)  │
  │   • Sequence matters: playlist must flow (tempo, mood, energy)       │
  │   • Passive listening: user can't skip every bad rec (in shower)    │
  │   • Context-dependent: workout music ≠ sleep music ≠ study music     │
  │   • Cold start for artists: millions of tracks have <1000 plays     │
  │                                                                       │
  │  1. DISCOVER WEEKLY (their iconic feature)                            │
  │                                                                       │
  │     30-song personalized playlist, refreshed every Monday.           │
  │     40 million users within first year. How it works:                │
  │                                                                       │
  │     Three signal sources combined:                                    │
  │                                                                       │
  │     a) Collaborative filtering:                                      │
  │        Matrix factorization on user-track matrix.                    │
  │        "Users with similar listening → recommend their tracks"       │
  │        Handles the "wisdom of crowds" signal.                        │
  │                                                                       │
  │     b) NLP on playlists ("Playlist2Vec"):                            │
  │        Treat each playlist as a "sentence", each track as a "word". │
  │        Train Word2Vec-style embeddings on billions of playlists.    │
  │        Result: tracks that appear in similar playlist contexts       │
  │        have similar embeddings.                                      │
  │        "Dark Side of the Moon" ≈ "Wish You Were Here" (Pink Floyd)  │
  │                                                                       │
  │     c) Audio analysis (for cold-start tracks):                       │
  │        Mel spectrogram → CNN → audio embedding (128-dim)             │
  │        Captures: tempo, energy, instruments, vocal style             │
  │        Can recommend brand new songs with 0 listens!                 │
  │        A song that SOUNDS like songs you like → recommended.         │
  │                                                                       │
  │     Filtering:                                                        │
  │        - Remove tracks user already knows (listened >2 times)        │
  │        - Remove tracks from artists already in user's library        │
  │        - Ensure genre/mood diversity within the 30 tracks            │
  │        - Freshness: mix in recent releases                           │
  │                                                                       │
  │  2. DAILY MIX (personalized radio stations)                           │
  │                                                                       │
  │     Cluster user's listening history into taste groups:               │
  │     - Daily Mix 1: indie rock (user's main taste)                    │
  │     - Daily Mix 2: electronic (secondary taste)                      │
  │     - Daily Mix 3: jazz (tertiary taste)                             │
  │                                                                       │
  │     Each mix: ~50 tracks, 70% familiar + 30% new                    │
  │     The 70/30 split is deliberately tuned for comfort + discovery.   │
  │                                                                       │
  │  3. RADIO / AUTOPLAY                                                  │
  │                                                                       │
  │     When a playlist/album ends, generate infinite continuation.      │
  │     Sequence model: given last N tracks, predict next track.         │
  │     Must maintain: mood, energy level, tempo continuity.             │
  │     Uses: transformer-based sequence model + audio feature matching. │
  │                                                                       │
  │  4. HOME PAGE                                                         │
  │                                                                       │
  │     Context-aware recommendations:                                    │
  │     Morning → upbeat, energetic playlists                            │
  │     Late night → chill, ambient music                                │
  │     Commute time → podcasts user hasn't finished                     │
  │     After workout → recovery playlist                                │
  │                                                                       │
  │     Time-of-day + day-of-week + recently played → contextual model.  │
  │                                                                       │
  │  5. PODCAST RECOMMENDATIONS                                          │
  │                                                                       │
  │     Different from music (episodes, not tracks):                     │
  │     - NLP on episode descriptions and transcripts                    │
  │     - Collaborative: "listeners of X also listen to Y"              │
  │     - Graph: topic → creator → listener bipartite graph              │
  │     - Vespa used for podcast search (confirmed by Spotify)           │
  │                                                                       │
  │  Infrastructure:                                                      │
  │   • Google Cloud Platform (migrated from on-prem 2018)              │
  │   • Luigi → Flyte for ML pipeline orchestration                     │
  │   • BigQuery + Dataflow for batch processing                         │
  │   • Custom feature store on Bigtable                                 │
  │   • TensorFlow + JAX for model training                              │
  │   • ~4 billion user-track interactions per day                       │
  └──────────────────────────────────────────────────────────────────────┘
```

### Amazon Product Recommendations

```
~35% of Amazon revenue comes from recommendations.
"Customers who bought X also bought Y" — the OG recommendation.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Amazon RecSys Architecture                        │
  │                                                                       │
  │  1. ITEM-BASED COLLABORATIVE FILTERING (the classic, 2003)           │
  │                                                                       │
  │     Amazon popularized item-item CF (not user-user):                 │
  │                                                                       │
  │     Pre-compute: for every item, find similar items                  │
  │       sim(item_A, item_B) = cosine of their co-purchase vectors      │
  │                                                                       │
  │     At serving time:                                                  │
  │       User viewed/bought items {X, Y, Z}                             │
  │       For each: lookup pre-computed similar items                    │
  │       Merge and rank → "Customers who bought X also bought..."       │
  │                                                                       │
  │     Why item-item (not user-user):                                   │
  │       • Items are more stable than users (tastes drift)              │
  │       • Item similarity can be pre-computed offline                  │
  │       • O(K) at serving time where K = user's recent items           │
  │       • Scales to hundreds of millions of items                      │
  │                                                                       │
  │  2. PERSONALIZED RANKINGS (evolved)                                   │
  │                                                                       │
  │     Product page:                                                     │
  │      "Frequently bought together" — association rules (lift/cosine)  │
  │      "Customers who viewed this also viewed" — co-view similarity    │
  │      "Compare with similar items" — attribute-based similarity       │
  │                                                                       │
  │     Homepage:                                                         │
  │      "Recommended for you" — deep learning model (2018+)             │
  │      Multi-source: purchase history + browse history + wishlist      │
  │      + search history + time-aware decay                             │
  │                                                                       │
  │     Email/notifications:                                              │
  │      "We think you'd like..." — batch predictions, sent daily       │
  │      "Back in stock" — purchase-intent based on past views           │
  │      "Price dropped" — items in wishlist/cart                        │
  │                                                                       │
  │  3. THE FLYWHEEL EFFECT                                               │
  │                                                                       │
  │     Better recs → more purchases → more data → better recs           │
  │     Amazon has unmatched PURCHASE data (not just clicks/views).      │
  │     Purchase signal is 10x more valuable than browse signal.         │
  │     This is their moat — no one else has this purchase history.      │
  │                                                                       │
  │  4. SEARCH + RECOMMENDATION INTEGRATION                              │
  │                                                                       │
  │     Search results ARE personalized:                                  │
  │     User A searches "headphones" → Bose (based on purchase history)  │
  │     User B searches "headphones" → budget brand (based on history)   │
  │     Search ranking = relevance × personalization × sponsored         │
  │                                                                       │
  │  5. SPONSORED PRODUCTS IN RECOMMENDATIONS                            │
  │                                                                       │
  │     Ads are blended into recommendation carousels.                   │
  │     "Inspired by your browsing history" may include sponsored items. │
  │     The ranking model jointly optimizes:                             │
  │       score = relevance × P(purchase) × ad_bid (if sponsored)       │
  │     This is why Amazon Ads is so profitable — it lives IN the rec.  │
  │                                                                       │
  │  Infrastructure:                                                      │
  │   • All on AWS (obviously)                                           │
  │   • Item-item similarity: pre-computed in Spark, stored in DynamoDB  │
  │   • Real-time features: DynamoDB + ElastiCache                       │
  │   • Deep learning models: SageMaker for training, custom serving    │
  │   • A9 (search) and recommendation are separate but integrated orgs │
  └──────────────────────────────────────────────────────────────────────┘
```

### Pinterest (Visual Discovery + Graph Neural Networks)

```
~500 million monthly active users. 300+ billion "Pins" (images).
Unique: visual-first, intent-rich (users actively planning/shopping).

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Pinterest RecSys Architecture                     │
  │                                                                       │
  │  1. PINSAGE (2018) — Graph Neural Networks at Scale                   │
  │                                                                       │
  │     The breakthrough: use the pin-board graph for embeddings.        │
  │                                                                       │
  │     ┌─────────────────────────────────────────────────┐              │
  │     │ Pin-Board bipartite graph:                       │              │
  │     │                                                  │              │
  │     │  Pin A ──── Board 1 ("Kitchen Ideas")            │              │
  │     │  Pin B ──── Board 1                              │              │
  │     │  Pin A ──── Board 2 ("Modern Homes")             │              │
  │     │  Pin C ──── Board 2                              │              │
  │     │                                                  │              │
  │     │  Pins on the same board = semantically related   │              │
  │     │  Pin A is related to both B and C (co-board)     │              │
  │     └─────────────────────────────────────────────────┘              │
  │                                                                       │
  │     PinSage: graph convolutional network on this graph.              │
  │      - Each pin starts with visual embedding (image CNN)             │
  │      - GCN aggregates neighbor embeddings (pins on same boards)      │
  │      - After K layers: pin embedding captures graph context          │
  │                                                                       │
  │     Scale challenge: 3 billion nodes, 18 billion edges.              │
  │      Can't do full-batch GCN. Solution:                              │
  │      - Random walk-based neighbor sampling                           │
  │      - Importance pooling (weight neighbors by visit count)          │
  │      - MapReduce-based training on CPU cluster                       │
  │      - Producer-consumer architecture for mini-batch generation      │
  │                                                                       │
  │     Result: 150-dim embedding per pin. Used for:                     │
  │      - "More like this" (ANN search for similar pins)                │
  │      - Home feed ranking                                             │
  │      - Ads targeting                                                  │
  │      - Shopping recommendations                                      │
  │                                                                       │
  │  2. VISUAL SEARCH                                                     │
  │                                                                       │
  │     User taps a region of a pin image:                               │
  │      - Crop detected object (object detection model)                 │
  │      - Encode cropped region → visual embedding                      │
  │      - ANN search over all pin visual embeddings                     │
  │      - Return visually similar pins                                  │
  │                                                                       │
  │     "I like this lamp in this living room photo → show me            │
  │      similar lamps I can buy"                                        │
  │                                                                       │
  │  3. HOME FEED (Pinnability model)                                    │
  │                                                                       │
  │     Predict P(user will pin/click/close-up this pin)                │
  │     Features: PinSage embedding, user history, pin freshness,       │
  │     creator quality, engagement stats, time of day.                  │
  │     Model: deep learning, multi-objective (pin, click, hide).       │
  │                                                                       │
  │     Feed construction:                                                │
  │      - Candidate sources: following, interests, related, trending   │
  │      - Ranking model scores all candidates                          │
  │      - Diversity injection: vary topics across the feed             │
  │      - Ads interleaved (shoppable pins = native ads)                │
  │                                                                       │
  │  Infrastructure:                                                      │
  │   • AWS (migrated from own DCs)                                      │
  │   • PinSage embeddings: pre-computed daily, stored in RocksDB       │
  │   • Real-time ranking: custom serving layer                          │
  │   • Training: PyTorch on GPU clusters                                │
  │   • Offline: Spark + Airflow for data pipelines                     │
  └──────────────────────────────────────────────────────────────────────┘
```

### Twitter / X (Timeline Ranking)

```
~500 million monthly active users. Timeline went from chronological → ranked (2016).

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Twitter RecSys Architecture                       │
  │  (open-sourced parts of the algorithm in 2023)                       │
  │                                                                       │
  │  1. TIMELINE ARCHITECTURE                                             │
  │                                                                       │
  │     Two timeline modes:                                               │
  │      "For You" (algorithmic, default) — ranked by model              │
  │      "Following" (chronological) — reverse-chronological of follows  │
  │                                                                       │
  │     "For You" pipeline:                                               │
  │                                                                       │
  │     ┌──────────────────────────────────────────────────────┐         │
  │     │ Candidate Sources (~1500 tweets)                      │         │
  │     │  In-Network (50%): tweets from people you follow     │         │
  │     │  Out-of-Network (50%): tweets you DON'T follow       │         │
  │     │    - Social graph: "your follows liked this tweet"   │         │
  │     │    - SimClusters: topically similar to your interests│         │
  │     │    - TweetSimilarity: embedding-based retrieval      │         │
  │     │    - Trending/popular tweets                         │         │
  │     └──────────────────────────────────────────────────────┘         │
  │                              │                                        │
  │                              ▼                                        │
  │     ┌──────────────────────────────────────────────────────┐         │
  │     │ Heavy Ranker (~48M parameter neural network)          │         │
  │     │                                                       │         │
  │     │  Predicts multiple engagement probabilities:          │         │
  │     │   P(favorite/like)   × 0.5                           │         │
  │     │   P(retweet)         × 1.0                           │         │
  │     │   P(reply)           × 1.0                           │         │
  │     │   P(click profile)   × ... (various weights)         │         │
  │     │   P(video watch 50%) × 0.005                         │         │
  │     │   P(report)          × -74.0  ← heavily penalized!  │         │
  │     │   P(negative feedback)× -74.0                        │         │
  │     │                                                       │         │
  │     │  Final score = weighted sum of all predictions        │         │
  │     │  Note: reports are penalized ~148x more than likes    │         │
  │     └──────────────────────────────────────────────────────┘         │
  │                              │                                        │
  │                              ▼                                        │
  │     ┌──────────────────────────────────────────────────────┐         │
  │     │ Heuristics & Filters                                  │         │
  │     │  - Author diversity (don't show 10 tweets from 1 user)│        │
  │     │  - Content diversity (mix topics)                     │         │
  │     │  - Visibility filtering (blocks, mutes, safety)      │         │
  │     │  - "Blue verified" boost (controversial, since 2023) │         │
  │     │  - Feedback-based demotion (hidden tweets demoted)   │         │
  │     └──────────────────────────────────────────────────────┘         │
  │                                                                       │
  │  2. SIMCLUSTERS (community detection for recommendations)             │
  │                                                                       │
  │     Factorize the user-user follow graph into ~145K communities.     │
  │     Each user has a sparse vector of community memberships:          │
  │       @elonmusk: {tech: 0.9, crypto: 0.7, space: 0.8, memes: 0.6}  │
  │                                                                       │
  │     Each tweet gets community scores based on who engages:           │
  │       tweet about SpaceX: {space: 0.9, tech: 0.7}                   │
  │                                                                       │
  │     Out-of-network recs: match user community vector with            │
  │     tweet community vector → high overlap = recommend.               │
  │                                                                       │
  │  3. TRUST AND SAFETY                                                  │
  │                                                                       │
  │     "Reputation score" per user (Tweepcred):                         │
  │     Based on: account age, follower/following ratio,                 │
  │     engagement quality, reports received.                            │
  │     Low-rep users' tweets get less distribution.                     │
  │                                                                       │
  │  Infrastructure (open-sourced, 2023):                                 │
  │   • Home Mixer: Scala-based pipeline orchestrator                    │
  │   • Earlybird: real-time tweet search index (custom inverted index)  │
  │   • Navi: ML model serving (TF, ONNX, Caffe2)                       │
  │   • Manhattan: internal distributed KV store (feature store)         │
  │   • SimClusters: Spark-based community detection, runs daily         │
  │   • Hosted on Google Cloud (migrated 2023-2024)                     │
  └──────────────────────────────────────────────────────────────────────┘
```

### LinkedIn Feed (Professional Context)

```
~1 billion members. Feed optimized for professional value, not just engagement.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     LinkedIn Feed Architecture                        │
  │                                                                       │
  │  What's different about LinkedIn:                                     │
  │   • Professional context: "viral" ≠ valuable (doomscrolling bad)    │
  │   • Creator ecosystem matters: must distribute to smaller creators   │
  │   • Quality over engagement: informative > entertaining              │
  │   • Connection-based: professional network graph is primary signal   │
  │                                                                       │
  │  Multi-objective ranking:                                             │
  │                                                                       │
  │   score = P(click) × w₁                                              │
  │         + P(like)  × w₂                                              │
  │         + P(comment) × w₃                                            │
  │         + P(share) × w₄                                              │
  │         + P(hide) × w₅ (negative)                                    │
  │         + creator_distribution_fairness × w₆                         │
  │                                                                       │
  │   The fairness term is unique to LinkedIn:                           │
  │    Without it: posts from viral creators dominate (power law)        │
  │    With it: spread distribution more evenly across creators          │
  │    Goal: every creator gets a baseline of views for quality content  │
  │                                                                       │
  │  "Knowledge" ranking signals:                                        │
  │   • Is this post informative vs. engagement-bait?                    │
  │   • Expert annotations + classifier for content quality              │
  │   • Dwell time: long reading time = genuinely engaging               │
  │   • "Follow" from post = strong signal of value                     │
  │                                                                       │
  │  Anti-viral measures:                                                 │
  │   • "Borderline" content classifier (engagement-bait detection)     │
  │   • Reshare chains capped (prevent runaway virality)                 │
  │   • "Meaningful conversations" metric: replies that add value        │
  │                                                                       │
  │  Infrastructure:                                                      │
  │   • Samza (streaming) for real-time feature computation              │
  │   • Espresso (custom document store) for profiles                    │
  │   • Venice (custom KV store) for ML features                         │
  │   • Pro-ML: internal ML platform for training + serving              │
  │   • Spark + HDFS for batch feature computation                       │
  └──────────────────────────────────────────────────────────────────────┘
```

## 12. System Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│              Full Recommendation System Architecture                     │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────┐            │
│  │ Data Collection                                          │            │
│  │  User events → Kafka → stream processing (Flink)        │            │
│  │  → Real-time feature updates (Redis/Memcached)           │            │
│  │  → Event log (S3/HDFS) for batch training                │            │
│  └─────────────────────────────────────────────────────────┘            │
│                              │                                           │
│  ┌─────────────────────────────────────────────────────────┐            │
│  │ Offline Training                                         │            │
│  │  Training data (Spark) → Model training (GPU cluster)    │            │
│  │  → Embedding generation → Item index build (FAISS/HNSW)  │            │
│  │  → Model validation → A/B test framework → Deploy        │            │
│  │  Cadence: daily or hourly                                │            │
│  └─────────────────────────────────────────────────────────┘            │
│                              │                                           │
│  ┌─────────────────────────────────────────────────────────┐            │
│  │ Online Serving (<200ms)                                  │            │
│  │                                                          │            │
│  │  Request → Feature Store lookup (Redis, <5ms)            │            │
│  │       → Candidate retrieval (ANN + CF + rules, <20ms)    │            │
│  │       → Pre-ranking (lightweight model, <10ms)            │            │
│  │       → Heavy ranking (deep model on GPU, <50ms)          │            │
│  │       → Re-ranking (diversity, business rules, <10ms)     │            │
│  │       → Response                                          │            │
│  └─────────────────────────────────────────────────────────┘            │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────┐            │
│  │ A/B Testing & Experimentation                            │            │
│  │  • Randomized user buckets (1-5% per experiment)         │            │
│  │  • Metrics pipeline: engagement, retention, revenue      │            │
│  │  • Statistical significance testing                      │            │
│  │  • Interleaving experiments (faster than A/B)            │            │
│  └─────────────────────────────────────────────────────────┘            │
└─────────────────────────────────────────────────────────────────────────┘
```

## 13. Common Pitfalls

```
┌──────────────────────────┬──────────────────────────────────────────────┐
│ Pitfall                  │ What goes wrong                              │
├──────────────────────────┼──────────────────────────────────────────────┤
│ Popularity bias          │ Always recommend popular items → new items   │
│                          │ never shown → rich get richer                │
│ Filter bubble            │ Only show similar to past → user never       │
│                          │ discovers new interests                      │
│ Position bias            │ Users click first result → model thinks      │
│                          │ position=1 is always relevant → self-fulfilling│
│ Feedback loop            │ Model → user behavior → training data →      │
│                          │ model confirms itself (amplification)        │
│ Selection bias           │ Only observe engagement on items shown       │
│                          │ (no data on items NOT shown)                 │
│ Exposure bias            │ Can't distinguish "user doesn't like" from   │
│                          │ "user never saw it"                          │
│ Short-term vs long-term  │ Optimizing clicks → addictive/clickbait     │
│                          │ content → lower long-term retention          │
└──────────────────────────┴──────────────────────────────────────────────┘

Position bias correction:
  - Inverse Propensity Weighting (IPW): weight training examples by
    1/P(shown at that position)
  - Position feature during training, remove at serving time
  - Swap experiments: randomly swap positions, observe change in CTR
```

## Numbers to Know

```
YouTube:    ~500 hours uploaded/minute, billions of recommendations/day
Netflix:    ~80% of content watched comes from recommendations
Amazon:     ~35% of revenue from recommendations
TikTok:     ~1B+ monthly active users, entirely rec-driven feed
Spotify:    Discover Weekly: 40M users within a year of launch

Latency:    <200ms end-to-end (including network)
Model size: embedding tables can be TBs (like DLRM)
Items:      millions to billions (YouTube videos, Amazon products)
Users:      billions (with varying activity levels)
Training:   daily or hourly model updates
Features:   100-1000 features per (user, item) pair
```

## Key Papers

| Paper | Year | Contribution |
|-------|------|-------------|
| Amazon Item-Item CF | 2003 | Scalable item-based collaborative filtering |
| Netflix Prize (SVD++) | 2009 | Matrix factorization for recommendations |
| YouTube DNN | 2016 | Two-stage deep neural network architecture |
| Wide & Deep | 2016 | Memorization + generalization for recs |
| DIN (Alibaba) | 2018 | Attention over user interest sequences |
| SASRec | 2018 | Transformer-based sequential recommendation |
| DLRM (Meta) | 2019 | Production-scale deep learning recommendation |
| DCN v2 (Google) | 2020 | Cross network for feature interactions |
| PinSage (Pinterest) | 2018 | Graph neural network on billion-scale graph |
| MMOE (Google) | 2018 | Multi-gate mixture of experts for multi-task |
