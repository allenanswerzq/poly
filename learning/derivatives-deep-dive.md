# Derivatives Deep Dive: From First Principles

A learning plan to understand derivatives (options, futures, swaps, perps) at a deep level.

---

## Part 1: Foundations (Week 1-2)

### 1.1 What is a Derivative?

A contract whose value **derives** from an underlying asset.

```
Underlying Assets:
├── Stocks (AAPL, TSLA)
├── Commodities (Oil, Gold, Wheat)
├── Currencies (USD/EUR)
├── Interest Rates (LIBOR, SOFR)
├── Crypto (BTC, ETH)
└── Indices (S&P 500, VIX)
```

### 1.2 Why Derivatives Exist

| Use Case | Example |
|----------|---------|
| **Hedging** | Farmer locks in wheat price before harvest |
| **Speculation** | Bet on BTC going up with 10x leverage |
| **Arbitrage** | Exploit price differences across markets |
| **Access** | Get exposure to assets you can't directly own |

### 1.3 Core Concepts to Master

- [ ] **Spot vs Forward Price**: Why are they different?
- [ ] **Time Value of Money**: Present value, discounting
- [ ] **No-Arbitrage Principle**: If two things have same payoff, they must have same price
- [ ] **Risk-Neutral Pricing**: Pricing as if everyone is risk-neutral
- [ ] **Replication**: Creating synthetic positions

---

## Part 2: Forwards & Futures (Week 2-3)

### 2.1 Forward Contracts

The simplest derivative: agree today to buy/sell at a fixed price in the future.

```
Today (t=0)                          Expiry (t=T)
    │                                     │
    │  Agree: Buy 1 BTC at $50,000        │  Settle:
    │  on Dec 31, 2026                    │  Pay $50,000
    │                                     │  Receive 1 BTC
    ▼                                     ▼
```

**Forward Price Formula:**
```
F = S₀ × e^(r×T)

Where:
  F  = Forward price
  S₀ = Spot price today
  r  = Risk-free rate
  T  = Time to expiry (years)
```

**Why?** Arbitrage argument:
- If F > S₀ × e^(rT): Borrow money, buy spot, sell forward → free money
- If F < S₀ × e^(rT): Short spot, lend money, buy forward → free money

### 2.2 Futures vs Forwards

| Feature | Forward | Future |
|---------|---------|--------|
| Trading | OTC (private) | Exchange-traded |
| Standardization | Custom terms | Standardized |
| Counterparty Risk | Yes | Clearinghouse guarantees |
| Settlement | At expiry | Daily mark-to-market |
| Margin | None or negotiated | Required |

### 2.3 Daily Mark-to-Market (Futures)

```
Day 0: Enter long future at $50,000
Day 1: Future price = $51,000 → +$1,000 credited to account
Day 2: Future price = $49,500 → -$1,500 debited from account
Day 3: Future price = $52,000 → +$2,500 credited to account
...
```

This daily settlement is why futures prices ≠ forward prices (convexity adjustment).

### 2.4 Key Topics

- [ ] Cost of carry model
- [ ] Contango vs Backwardation
- [ ] Basis and basis risk
- [ ] Rolling futures positions
- [ ] Calendar spreads

---

## Part 3: Perpetual Futures (Perps) (Week 3-4)

### 3.1 The Innovation

Traditional futures expire. Perps never expire. But how?

**Problem**: Without expiry, perp price could diverge from spot forever.

**Solution**: Funding rate mechanism.

### 3.2 Funding Rate Deep Dive

```
Every 8 hours (typically):

If Perp Price > Spot Price:
  → Longs pay Shorts
  → Incentivizes shorts, pushes perp price down

If Perp Price < Spot Price:
  → Shorts pay Longs
  → Incentivizes longs, pushes perp price up
```

**Funding Rate Formula (simplified):**
```
Funding Rate = (Perp Price - Spot Price) / Spot Price × Multiplier

Example:
  Spot  = $50,000
  Perp  = $50,500
  Rate  = ($50,500 - $50,000) / $50,000 = 0.01 = 1%

  If you're long $100,000 notional:
  You pay: $100,000 × 1% = $1,000 to shorts
```

### 3.3 Why Funding Rate Works (Game Theory)

```
Scenario: Perp trading at premium (perp > spot)

Arbitrageur sees opportunity:
  1. Short perp at $50,500
  2. Buy spot at $50,000

This position:
  - Delta neutral (no directional risk)
  - Collects funding every 8h (longs pay shorts)
  - Profit = Funding payments - Trading costs

As more arbs do this:
  - Shorting pressure pushes perp down
  - Buying pressure pushes spot up
  - Prices converge!
```

### 3.4 Perps on DeFi vs CeFi

| Aspect | CeFi (Binance, Bybit) | DeFi (dYdX, GMX) |
|--------|----------------------|------------------|
| Order Book | Central limit order book | AMM or virtual AMM |
| Oracle | Internal price feeds | Chainlink, Pyth |
| Liquidation | Centralized engine | Smart contract |
| Funding | Every 8h | Continuous or hourly |
| Counterparty | Exchange | LP pool |

### 3.5 Key Topics

- [ ] How oracles work (Chainlink, Pyth, TWAP)
- [ ] Liquidation mechanisms and cascades
- [ ] Insurance funds
- [ ] Virtual AMMs (vAMM) - dYdX v3, Perp Protocol
- [ ] LP-based perps (GMX, Gains Network)
- [ ] Funding rate arbitrage strategies

---

## Part 4: Options (Week 4-6)

### 4.1 Fundamentals

```
Call Option: Right to BUY at strike price
Put Option:  Right to SELL at strike price

Key Parameters:
  S = Spot price (current)
  K = Strike price
  T = Time to expiry
  r = Risk-free rate
  σ = Volatility
```

### 4.2 Payoff Diagrams

```
Long Call (K=100):              Long Put (K=100):
     │                               │
  P  │          ╱                 P  │ ╲
  a  │        ╱                   a  │   ╲
  y  │      ╱                     y  │     ╲
  o  │────●                       o  │       ●────
  f  │    K=100                   f  │       K=100
  f  │                            f  │
     └────────────────               └────────────────
              Spot Price                      Spot Price

Long Call Payoff = max(S - K, 0)
Long Put Payoff  = max(K - S, 0)
```

### 4.3 Put-Call Parity

One of the most important relationships:

```
C - P = S - K × e^(-rT)

Where:
  C = Call price
  P = Put price
  S = Spot price
  K = Strike price

Rearranged:
  C + K×e^(-rT) = P + S

"Call + Bond = Put + Stock"
```

**Why?** Both sides have identical payoffs at expiry. If not equal, arbitrage.

### 4.4 Black-Scholes Model

The Nobel Prize-winning formula:

```
C = S × N(d₁) - K × e^(-rT) × N(d₂)
P = K × e^(-rT) × N(-d₂) - S × N(-d₁)

Where:
  d₁ = [ln(S/K) + (r + σ²/2)T] / (σ√T)
  d₂ = d₁ - σ√T
  N(x) = Standard normal CDF
```

**Key Assumptions:**
1. Log-normal price distribution
2. Constant volatility (σ)
3. No dividends
4. No transaction costs
5. Continuous trading
6. Risk-free rate constant

**What each term means:**
- `S × N(d₁)`: Delta-weighted stock position
- `K × e^(-rT) × N(d₂)`: Present value of strike × probability of exercise

### 4.5 The Greeks

Sensitivities of option price to various factors:

| Greek | Symbol | Measures | Formula (Call) |
|-------|--------|----------|----------------|
| **Delta** | Δ | Price sensitivity | N(d₁) |
| **Gamma** | Γ | Delta sensitivity | N'(d₁) / (S×σ×√T) |
| **Theta** | Θ | Time decay | -(S×N'(d₁)×σ)/(2√T) - rKe^(-rT)N(d₂) |
| **Vega** | ν | Volatility sensitivity | S×√T×N'(d₁) |
| **Rho** | ρ | Interest rate sensitivity | KTe^(-rT)N(d₂) |

```
Delta (Δ):
  ├── Call: 0 to 1 (typically 0.5 at-the-money)
  ├── Put: -1 to 0
  └── Interpretation: "Equivalent stock position"
      Delta 0.5 = 100 calls behave like 50 shares

Gamma (Γ):
  ├── Always positive for long options
  ├── Highest at-the-money, near expiry
  └── Why it matters: Your delta changes as price moves

Theta (Θ):
  ├── Usually negative (time decay hurts option buyers)
  ├── Accelerates near expiry
  └── "Rent" you pay to hold optionality

Vega (ν):
  ├── Always positive for long options
  ├── Highest at-the-money
  └── Why IV matters more than you think
```

### 4.6 Implied Volatility (IV)

The market's expectation of future volatility, backed out from option prices.

```
Given: Option price in market
Black-Scholes: C = f(S, K, T, r, σ)

Solve for σ such that f(S, K, T, r, σ) = Market Price

This σ is the Implied Volatility
```

**IV Smile/Skew:**
```
IV
 │    ╲         ╱
 │     ╲       ╱
 │      ╲_____╱
 │         │
 └─────────┼─────────── Strike
          ATM

- OTM puts have higher IV (crash protection demand)
- OTM calls may have higher IV (speculation demand)
- This violates Black-Scholes assumption of constant σ!
```

### 4.7 Key Topics

- [ ] Binomial option pricing (discrete version of B-S)
- [ ] Risk-neutral valuation derivation
- [ ] American vs European options
- [ ] Early exercise (when is it optimal?)
- [ ] Exotic options: Barriers, Asians, Lookbacks
- [ ] Volatility surface and term structure
- [ ] Delta hedging in practice
- [ ] Gamma scalping

---

## Part 5: Swaps (Week 6-7)

### 5.1 Interest Rate Swaps

```
Fixed-for-Floating Swap:

Party A                              Party B
(pays fixed)                         (pays floating)
    │                                     │
    │──────── 5% fixed ──────────────────▶│
    │                                     │
    │◀────── SOFR + spread ──────────────│
    │                                     │

Notional: $100M
Tenor: 5 years
```

**Why use swaps?**
- Company has floating-rate debt but wants fixed payments
- Bank has fixed-rate assets but floating liabilities
- Speculation on interest rate movements

### 5.2 Swap Valuation

A swap is a series of forward contracts:

```
Swap Value = Σ (Forward Rate - Fixed Rate) × Notional × Δt × Discount Factor

At inception: Swap value = 0 (rates are set so both sides are "fair")
Over time: Swap gains/loses value as rates move
```

### 5.3 Other Swap Types

| Type | Exchange |
|------|----------|
| Currency Swap | USD cash flows ↔ EUR cash flows |
| Equity Swap | Fixed/floating ↔ Stock returns |
| Commodity Swap | Fixed price ↔ Floating commodity price |
| Total Return Swap | Financing rate ↔ Total return of asset |
| Credit Default Swap | Premium payments ↔ Default protection |

### 5.4 Key Topics

- [ ] Forward rate agreements (FRAs)
- [ ] Swap curve construction
- [ ] Basis swaps
- [ ] Cross-currency basis
- [ ] CVA/DVA (credit adjustments)

---

## Part 6: On-Chain Derivatives (Week 7-8)

### 6.1 DeFi Options Protocols

| Protocol | Model | Chain |
|----------|-------|-------|
| Dopex | SSOV (Single Staking Option Vaults) | Arbitrum |
| Lyra | AMM with dynamic pricing | Optimism, Arbitrum |
| Premia | Peer-to-pool options | Multi-chain |
| Panoptic | Uniswap LP positions as options | Ethereum |
| Aevo | Off-chain orderbook, on-chain settlement | Ethereum L2 |

### 6.2 Panoptic: LP Positions as Options

Revolutionary insight: Uniswap LP positions have option-like payoffs!

```
Traditional Option:
  Seller receives premium upfront
  Takes on obligation if exercised

Uniswap LP:
  LP provides liquidity
  Collects fees (like premium)
  Position value changes with price (like option payoff)

Panoptic:
  Wraps Uni V3 positions
  Creates "Panoptions" - perpetual options
  No expiry, no oracle needed
```

### 6.3 Squeeth (Opyn)

Perpetual contract that tracks ETH²

```
Squeeth = Squared ETH = Constant exposure to ETH²

Why?
  - Options require choosing strike and expiry
  - Squeeth gives you "pure gamma" - convex exposure
  - Always long volatility

Payoff:
  If ETH goes from $1000 to $1100 (+10%):
  Squeeth goes up ~20% (approximately 2x leverage at ATM)
```

### 6.4 Key DeFi Challenges

- [ ] Oracle manipulation risks
- [ ] Liquidity fragmentation
- [ ] Gas costs vs premium
- [ ] Smart contract risk
- [ ] Capital efficiency

---

## Part 7: Advanced Topics (Week 8+)

### 7.1 Volatility Trading

```
Volatility Products:
├── VIX futures and options
├── Variance swaps
├── Volatility swaps
├── Straddles/Strangles
└── Correlation swaps
```

**Variance Swap:**
```
Payoff = Notional × (Realized Variance - Strike Variance)

Realized Variance = (252/n) × Σ(ln(Sᵢ/Sᵢ₋₁))²

Pure bet on volatility, not direction
```

### 7.2 Exotic Options

| Type | Description |
|------|-------------|
| **Barrier** | Activated/deactivated when price hits level |
| **Asian** | Payoff based on average price |
| **Lookback** | Payoff based on max/min price |
| **Digital/Binary** | Fixed payoff if condition met |
| **Compound** | Option on an option |
| **Rainbow** | Based on multiple underlyings |

### 7.3 Structured Products

```
Principal Protected Note:
├── Buy zero-coupon bond (guarantees principal)
└── Use remaining premium to buy call options

Autocallable:
├── If price > barrier at observation: Early redemption + coupon
├── If price stays in range: Collect coupons
└── If price < barrier at maturity: Lose principal

Worst-of Options:
└── Payoff based on worst-performing asset in basket
```

### 7.4 Risk Management

- [ ] Value at Risk (VaR)
- [ ] Expected Shortfall (CVaR)
- [ ] Stress testing
- [ ] Scenario analysis
- [ ] Greeks aggregation and limits
- [ ] Counterparty credit risk

---

## Reading List

### Books (in order)

1. **"Options, Futures, and Other Derivatives"** - John Hull
   - The bible. Read chapters 1-15 first.

2. **"Option Volatility and Pricing"** - Sheldon Natenberg
   - Practical trading perspective

3. **"Dynamic Hedging"** - Nassim Taleb
   - Real-world risk management

4. **"Volatility Surface"** - Jim Gatheral
   - Deep dive into vol modeling

5. **"Paul Wilmott on Quantitative Finance"** - Paul Wilmott
   - Advanced math, 3 volumes

### Papers

- Black, Scholes (1973) - "The Pricing of Options and Corporate Liabilities"
- Merton (1973) - "Theory of Rational Option Pricing"
- Cox, Ross, Rubinstein (1979) - "Option Pricing: A Simplified Approach" (Binomial model)
- Heston (1993) - "A Closed-Form Solution for Options with Stochastic Volatility"

### DeFi Specific

- dYdX documentation: https://docs.dydx.exchange/
- GMX documentation: https://docs.gmx.io/
- Panoptic research: https://panoptic.xyz/research
- Squeeth documentation: https://squeeth.opyn.co/

---

## Project Ideas (Build to Learn)

### Beginner
1. **Option Pricer**: Implement Black-Scholes from scratch
2. **Greeks Calculator**: Compute and visualize all Greeks
3. **Payoff Visualizer**: Interactive diagrams for any strategy

### Intermediate
4. **Binomial Tree**: Price American options
5. **Monte Carlo Pricer**: Price path-dependent options
6. **Volatility Surface Fitter**: Build IV surface from market data
7. **Delta Hedging Simulator**: Simulate P&L of delta-neutral strategy

### Advanced
8. **Perp DEX**: Build a simple perpetual futures exchange
9. **Options AMM**: Implement Lyra-style pricing
10. **Funding Rate Arbitrage Bot**: Spot-perp basis trade
11. **Volatility Forecasting**: GARCH, realized vol prediction

---

## Timeline Summary

| Week | Topic | Key Deliverable |
|------|-------|-----------------|
| 1-2 | Foundations + Forwards | Forward pricer |
| 2-3 | Futures | Basis trading simulator |
| 3-4 | Perps | Funding rate analyzer |
| 4-6 | Options | Black-Scholes implementation + Greeks |
| 6-7 | Swaps | Swap valuation |
| 7-8 | DeFi Derivatives | Study one protocol deeply |
| 8+ | Advanced | Pick specialization |

---

*"The only way to learn derivatives is to trade them (on paper first)."*
