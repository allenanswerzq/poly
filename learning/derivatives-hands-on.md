# Derivatives Hands-On: Learn by Building

No theory lectures. Just build stuff and understand through code.

---

## Project 1: Build a Simple Order Book (Week 1)

Understand how exchanges match trades before anything else.

### 1.1 Create the Order Book

```python
# orderbook.py
from dataclasses import dataclass
from enum import Enum
from typing import Optional
import heapq
import time

class Side(Enum):
    BID = "bid"
    ASK = "ask"

@dataclass
class Order:
    id: str
    price: float
    size: float
    side: Side
    timestamp: float

    def __lt__(self, other):
        # For bids: higher price = higher priority
        # For asks: lower price = higher priority
        if self.side == Side.BID:
            return (-self.price, self.timestamp) < (-other.price, other.timestamp)
        else:
            return (self.price, self.timestamp) < (other.price, other.timestamp)

class OrderBook:
    def __init__(self):
        self.bids: list[Order] = []  # max-heap (negate prices)
        self.asks: list[Order] = []  # min-heap
        self.orders: dict[str, Order] = {}
        self._order_counter = 0

    def add_order(self, price: float, size: float, side: Side) -> str:
        self._order_counter += 1
        order_id = f"order_{self._order_counter}"
        order = Order(order_id, price, size, side, time.time())

        self.orders[order_id] = order

        if side == Side.BID:
            heapq.heappush(self.bids, order)
        else:
            heapq.heappush(self.asks, order)

        # Try to match
        self._match()

        return order_id

    def _match(self):
        """Match crossing orders"""
        while self.bids and self.asks:
            best_bid = self.bids[0]
            best_ask = self.asks[0]

            # Check if orders cross
            if best_bid.price >= best_ask.price:
                # Match at the resting order's price (price-time priority)
                match_price = best_ask.price  # Taker crosses, maker price wins
                match_size = min(best_bid.size, best_ask.size)

                print(f"MATCH: {match_size} @ {match_price}")

                best_bid.size -= match_size
                best_ask.size -= match_size

                if best_bid.size <= 0:
                    heapq.heappop(self.bids)
                    del self.orders[best_bid.id]

                if best_ask.size <= 0:
                    heapq.heappop(self.asks)
                    del self.orders[best_ask.id]
            else:
                break  # No more crossing orders

    def get_best_bid(self) -> Optional[float]:
        return self.bids[0].price if self.bids else None

    def get_best_ask(self) -> Optional[float]:
        return self.asks[0].price if self.asks else None

    def get_mid_price(self) -> Optional[float]:
        bid, ask = self.get_best_bid(), self.get_best_ask()
        if bid and ask:
            return (bid + ask) / 2
        return None

    def get_spread(self) -> Optional[float]:
        bid, ask = self.get_best_bid(), self.get_best_ask()
        if bid and ask:
            return ask - bid
        return None

    def print_book(self, depth: int = 5):
        print("\n--- Order Book ---")
        print("ASKS:")
        asks_sorted = sorted([o for o in self.asks if o.size > 0],
                            key=lambda x: x.price, reverse=True)[:depth]
        for o in asks_sorted:
            print(f"  {o.price:.2f} | {o.size:.4f}")
        print(f"  --- spread: {self.get_spread():.2f} ---")
        print("BIDS:")
        bids_sorted = sorted([o for o in self.bids if o.size > 0],
                            key=lambda x: x.price, reverse=True)[:depth]
        for o in bids_sorted:
            print(f"  {o.price:.2f} | {o.size:.4f}")


# Test it
if __name__ == "__main__":
    book = OrderBook()

    # Add some orders
    book.add_order(100.00, 1.0, Side.BID)
    book.add_order(99.50, 2.0, Side.BID)
    book.add_order(101.00, 1.5, Side.ASK)
    book.add_order(101.50, 0.5, Side.ASK)

    book.print_book()

    # This should match!
    print("\n--- Adding crossing order ---")
    book.add_order(101.00, 0.5, Side.BID)  # Crosses the ask

    book.print_book()
```

### 1.2 Exercises

1. Add cancel order functionality
2. Add market orders (execute at best available)
3. Track trade history
4. Add order book snapshots/L2 data
5. Simulate latency/network delays

---

## Project 2: Build a Market Maker Bot (Week 2)

### 2.1 Simple Market Maker

```python
# market_maker.py
import random
from orderbook import OrderBook, Side

class SimpleMarketMaker:
    """
    Basic market maker strategy:
    - Quote bid and ask around fair price
    - Earn the spread when both sides get hit
    - Manage inventory risk
    """

    def __init__(self, orderbook: OrderBook, initial_capital: float = 10000):
        self.book = orderbook
        self.capital = initial_capital
        self.inventory = 0.0  # Positive = long, negative = short
        self.pnl = 0.0
        self.trades = []

        # Parameters
        self.spread_bps = 20  # 20 basis points = 0.2%
        self.order_size = 0.1
        self.max_inventory = 5.0  # Max position size

        # Track our orders
        self.bid_order_id = None
        self.ask_order_id = None

    def get_fair_price(self) -> float:
        """
        In real life, this is THE hard problem.
        Here we just use mid price.
        """
        mid = self.book.get_mid_price()
        if mid:
            return mid
        return 100.0  # Default

    def calculate_quotes(self) -> tuple[float, float]:
        """Calculate bid and ask prices"""
        fair = self.get_fair_price()
        half_spread = fair * (self.spread_bps / 10000) / 2

        bid = fair - half_spread
        ask = fair + half_spread

        # Skew quotes based on inventory
        # If we're long, we want to sell more -> lower ask
        # If we're short, we want to buy more -> higher bid
        inventory_skew = self.inventory * 0.01  # 1 cent per unit of inventory
        bid -= inventory_skew
        ask -= inventory_skew

        return bid, ask

    def update_quotes(self):
        """Cancel old orders and place new ones"""
        # Cancel existing orders
        # (In real impl, you'd cancel through the exchange)

        bid_price, ask_price = self.calculate_quotes()

        # Adjust size based on inventory limits
        bid_size = self.order_size if self.inventory < self.max_inventory else 0
        ask_size = self.order_size if self.inventory > -self.max_inventory else 0

        if bid_size > 0:
            self.bid_order_id = self.book.add_order(bid_price, bid_size, Side.BID)

        if ask_size > 0:
            self.ask_order_id = self.book.add_order(ask_price, ask_size, Side.ASK)

        print(f"MM quotes: BID {bid_price:.2f} x {bid_size} | ASK {ask_price:.2f} x {ask_size}")
        print(f"Inventory: {self.inventory:.2f}, PnL: ${self.pnl:.2f}")

    def on_fill(self, side: Side, price: float, size: float):
        """Called when our order gets filled"""
        if side == Side.BID:
            self.inventory += size
            self.capital -= price * size
        else:
            self.inventory -= size
            self.capital += price * size

        self.trades.append({
            'side': side,
            'price': price,
            'size': size,
            'inventory_after': self.inventory
        })

    def calculate_pnl(self, current_price: float) -> float:
        """Mark-to-market PnL"""
        return self.capital + self.inventory * current_price - 10000


# Simulate market making
def simulate_market():
    book = OrderBook()
    mm = SimpleMarketMaker(book)

    # Initialize book with some liquidity
    for i in range(5):
        book.add_order(99.0 - i * 0.1, random.uniform(0.5, 2.0), Side.BID)
        book.add_order(101.0 + i * 0.1, random.uniform(0.5, 2.0), Side.ASK)

    # Simulate 100 ticks
    for tick in range(100):
        print(f"\n=== Tick {tick} ===")

        # MM updates quotes
        mm.update_quotes()

        # Random market order comes in
        if random.random() < 0.3:  # 30% chance of trade
            side = random.choice([Side.BID, Side.ASK])
            size = random.uniform(0.05, 0.3)

            if side == Side.BID:
                # Market buy -> hits asks
                if book.get_best_ask():
                    print(f"Market BUY {size:.2f} @ market")
                    book.add_order(999999, size, Side.BID)  # Price high enough to cross
            else:
                # Market sell -> hits bids
                if book.get_best_bid():
                    print(f"Market SELL {size:.2f} @ market")
                    book.add_order(0.01, size, Side.ASK)  # Price low enough to cross

        book.print_book()

if __name__ == "__main__":
    simulate_market()
```

### 2.2 Exercises

1. Add realistic fill tracking (when MM's orders get hit)
2. Implement inventory-based quote skewing
3. Add fee calculation (maker/taker fees)
4. Track Sharpe ratio of MM strategy
5. Add adverse selection simulation (toxic flow)

---

## Project 3: Build a Perpetual Exchange (Week 3-4)

### 3.1 Perp Contract Core

```python
# perp_exchange.py
from dataclasses import dataclass
from typing import Dict, Optional
import time

@dataclass
class Position:
    size: float  # Positive = long, negative = short
    entry_price: float
    margin: float  # Collateral
    last_funding_time: float

@dataclass
class PerpMarket:
    index_price: float  # Oracle price (spot)
    mark_price: float   # Exchange's calculated price
    funding_rate: float  # Current funding rate
    last_funding_time: float
    open_interest_long: float
    open_interest_short: float

class PerpExchange:
    def __init__(self):
        self.positions: Dict[str, Position] = {}
        self.market = PerpMarket(
            index_price=50000.0,
            mark_price=50000.0,
            funding_rate=0.0,
            last_funding_time=time.time(),
            open_interest_long=0.0,
            open_interest_short=0.0
        )
        self.balances: Dict[str, float] = {}

        # Parameters
        self.maintenance_margin_ratio = 0.05  # 5%
        self.initial_margin_ratio = 0.10     # 10% (10x max leverage)
        self.funding_interval = 8 * 3600     # 8 hours in seconds
        self.max_funding_rate = 0.01         # 1% max per interval

    def deposit(self, user: str, amount: float):
        """Deposit collateral"""
        self.balances[user] = self.balances.get(user, 0) + amount
        print(f"{user} deposited ${amount:.2f}")

    def open_position(self, user: str, size: float, leverage: float) -> bool:
        """
        Open a position
        size > 0 = long, size < 0 = short
        """
        if user in self.positions:
            print("Already has position, use increase/decrease")
            return False

        notional = abs(size) * self.market.mark_price
        required_margin = notional / leverage

        if required_margin > self.balances.get(user, 0):
            print(f"Insufficient margin. Need ${required_margin:.2f}")
            return False

        if leverage > 1 / self.initial_margin_ratio:
            print(f"Max leverage is {1/self.initial_margin_ratio}x")
            return False

        # Deduct margin from balance
        self.balances[user] -= required_margin

        # Create position
        self.positions[user] = Position(
            size=size,
            entry_price=self.market.mark_price,
            margin=required_margin,
            last_funding_time=time.time()
        )

        # Update open interest
        if size > 0:
            self.market.open_interest_long += abs(size)
        else:
            self.market.open_interest_short += abs(size)

        print(f"{user} opened {size:.4f} @ ${self.market.mark_price:.2f} "
              f"with ${required_margin:.2f} margin ({leverage}x)")
        return True

    def close_position(self, user: str) -> Optional[float]:
        """Close position and return PnL"""
        if user not in self.positions:
            print("No position to close")
            return None

        pos = self.positions[user]

        # Calculate PnL
        if pos.size > 0:  # Long
            pnl = (self.market.mark_price - pos.entry_price) * pos.size
        else:  # Short
            pnl = (pos.entry_price - self.market.mark_price) * abs(pos.size)

        # Return margin + PnL to balance
        self.balances[user] += pos.margin + pnl

        # Update open interest
        if pos.size > 0:
            self.market.open_interest_long -= abs(pos.size)
        else:
            self.market.open_interest_short -= abs(pos.size)

        del self.positions[user]

        print(f"{user} closed position. PnL: ${pnl:.2f}")
        return pnl

    def calculate_funding_rate(self) -> float:
        """
        Funding rate = (Mark Price - Index Price) / Index Price

        Clamped to max rate
        """
        premium = (self.market.mark_price - self.market.index_price) / self.market.index_price

        # Clamp to max
        rate = max(-self.max_funding_rate, min(self.max_funding_rate, premium))

        return rate

    def apply_funding(self):
        """
        Apply funding payments between longs and shorts

        If funding rate > 0: Longs pay shorts
        If funding rate < 0: Shorts pay longs
        """
        rate = self.calculate_funding_rate()
        self.market.funding_rate = rate

        print(f"\n=== Funding Settlement ===")
        print(f"Index: ${self.market.index_price:.2f}, Mark: ${self.market.mark_price:.2f}")
        print(f"Funding rate: {rate * 100:.4f}%")

        for user, pos in self.positions.items():
            # Funding payment = position size * mark price * funding rate
            notional = pos.size * self.market.mark_price
            payment = notional * rate

            # Longs pay positive funding, shorts receive it
            pos.margin -= payment

            if payment > 0:
                print(f"  {user}: PAID ${payment:.2f} (long)")
            else:
                print(f"  {user}: RECEIVED ${-payment:.2f} (short)")

        self.market.last_funding_time = time.time()

    def get_liquidation_price(self, user: str) -> Optional[float]:
        """Calculate price at which position gets liquidated"""
        if user not in self.positions:
            return None

        pos = self.positions[user]

        # Liquidation when: margin + unrealized PnL < maintenance margin
        # maintenance margin = |size| * price * maintenance_ratio
        #
        # For long: margin + (price - entry) * size = size * price * maint_ratio
        # Solving: price = (margin + entry * size) / (size * (1 + maint_ratio))
        #
        # For short: margin + (entry - price) * |size| = |size| * price * maint_ratio
        # Solving: price = (entry * |size| + margin) / (|size| * (1 + maint_ratio))

        if pos.size > 0:  # Long
            liq_price = (pos.entry_price * pos.size - pos.margin) / (pos.size * (1 - self.maintenance_margin_ratio))
        else:  # Short
            liq_price = (pos.entry_price * abs(pos.size) + pos.margin) / (abs(pos.size) * (1 + self.maintenance_margin_ratio))

        return max(0, liq_price)

    def check_liquidations(self):
        """Check and liquidate underwater positions"""
        to_liquidate = []

        for user, pos in self.positions.items():
            liq_price = self.get_liquidation_price(user)

            if pos.size > 0 and self.market.mark_price <= liq_price:
                to_liquidate.append(user)
            elif pos.size < 0 and self.market.mark_price >= liq_price:
                to_liquidate.append(user)

        for user in to_liquidate:
            print(f"!!! LIQUIDATING {user} !!!")
            pos = self.positions[user]
            # In real exchange, there's a liquidation engine
            # Here we just close at current price
            del self.positions[user]
            # Margin goes to insurance fund
            print(f"  Position closed, margin ${pos.margin:.2f} forfeited")

    def update_price(self, new_index: float, new_mark: float):
        """Simulate price update (in real life, from oracle + orderbook)"""
        self.market.index_price = new_index
        self.market.mark_price = new_mark
        self.check_liquidations()

    def print_status(self):
        print(f"\n=== Exchange Status ===")
        print(f"Index: ${self.market.index_price:.2f}")
        print(f"Mark:  ${self.market.mark_price:.2f}")
        print(f"OI Long:  {self.market.open_interest_long:.4f}")
        print(f"OI Short: {self.market.open_interest_short:.4f}")
        print(f"Funding:  {self.market.funding_rate * 100:.4f}%")
        print(f"\nPositions:")
        for user, pos in self.positions.items():
            side = "LONG" if pos.size > 0 else "SHORT"
            liq = self.get_liquidation_price(user)
            pnl = (self.market.mark_price - pos.entry_price) * pos.size
            print(f"  {user}: {side} {abs(pos.size):.4f} @ ${pos.entry_price:.2f}, "
                  f"Margin: ${pos.margin:.2f}, PnL: ${pnl:.2f}, Liq: ${liq:.2f}")


# Demo
if __name__ == "__main__":
    exchange = PerpExchange()

    # Users deposit
    exchange.deposit("alice", 10000)
    exchange.deposit("bob", 10000)
    exchange.deposit("charlie", 5000)

    # Open positions
    exchange.open_position("alice", 0.5, 10)   # Long 0.5 BTC at 10x
    exchange.open_position("bob", -0.3, 5)     # Short 0.3 BTC at 5x
    exchange.open_position("charlie", 0.2, 20) # Long 0.2 BTC at 20x (high leverage)

    exchange.print_status()

    # Price moves up
    print("\n--- Price moves to $52,000 ---")
    exchange.update_price(52000, 52500)  # Mark > Index = premium
    exchange.print_status()

    # Funding settlement
    exchange.apply_funding()

    # Price crashes
    print("\n--- Price crashes to $45,000 ---")
    exchange.update_price(45000, 44500)
    exchange.print_status()

    # Close remaining positions
    for user in list(exchange.positions.keys()):
        exchange.close_position(user)
```

### 3.2 Exercises

1. Add the order book from Project 1 for price discovery
2. Implement the mark price calculation (TWAP of order book)
3. Add insurance fund for handling liquidations
4. Implement ADL (Auto-Deleveraging) when insurance fund depleted
5. Add partial liquidations
6. Build a funding rate arbitrage bot

---

## Project 4: Build an Options Pricer (Week 4-5)

### 4.1 Black-Scholes from Scratch

```python
# options.py
import math
from dataclasses import dataclass
from enum import Enum
from typing import Tuple

class OptionType(Enum):
    CALL = "call"
    PUT = "put"

def norm_cdf(x: float) -> float:
    """Standard normal CDF - approximate"""
    # Abramowitz and Stegun approximation
    a1 =  0.254829592
    a2 = -0.284496736
    a3 =  1.421413741
    a4 = -1.453152027
    a5 =  1.061405429
    p  =  0.3275911

    sign = 1 if x >= 0 else -1
    x = abs(x)

    t = 1.0 / (1.0 + p * x)
    y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * math.exp(-x * x / 2)

    return 0.5 * (1.0 + sign * y)

def norm_pdf(x: float) -> float:
    """Standard normal PDF"""
    return math.exp(-x * x / 2) / math.sqrt(2 * math.pi)

@dataclass
class OptionParams:
    S: float      # Spot price
    K: float      # Strike price
    T: float      # Time to expiry (years)
    r: float      # Risk-free rate
    sigma: float  # Volatility

def black_scholes_d1_d2(params: OptionParams) -> Tuple[float, float]:
    """Calculate d1 and d2"""
    S, K, T, r, sigma = params.S, params.K, params.T, params.r, params.sigma

    d1 = (math.log(S / K) + (r + sigma**2 / 2) * T) / (sigma * math.sqrt(T))
    d2 = d1 - sigma * math.sqrt(T)

    return d1, d2

def black_scholes_price(params: OptionParams, option_type: OptionType) -> float:
    """Calculate option price using Black-Scholes"""
    S, K, T, r = params.S, params.K, params.T, params.r
    d1, d2 = black_scholes_d1_d2(params)

    if option_type == OptionType.CALL:
        price = S * norm_cdf(d1) - K * math.exp(-r * T) * norm_cdf(d2)
    else:  # PUT
        price = K * math.exp(-r * T) * norm_cdf(-d2) - S * norm_cdf(-d1)

    return price

def calculate_greeks(params: OptionParams, option_type: OptionType) -> dict:
    """Calculate all Greeks"""
    S, K, T, r, sigma = params.S, params.K, params.T, params.r, params.sigma
    d1, d2 = black_scholes_d1_d2(params)

    sqrt_T = math.sqrt(T)
    exp_neg_rT = math.exp(-r * T)

    # Delta
    if option_type == OptionType.CALL:
        delta = norm_cdf(d1)
    else:
        delta = norm_cdf(d1) - 1

    # Gamma (same for call and put)
    gamma = norm_pdf(d1) / (S * sigma * sqrt_T)

    # Theta
    theta_common = -(S * norm_pdf(d1) * sigma) / (2 * sqrt_T)
    if option_type == OptionType.CALL:
        theta = theta_common - r * K * exp_neg_rT * norm_cdf(d2)
    else:
        theta = theta_common + r * K * exp_neg_rT * norm_cdf(-d2)
    theta = theta / 365  # Convert to daily

    # Vega (same for call and put)
    vega = S * sqrt_T * norm_pdf(d1) / 100  # Per 1% move in vol

    # Rho
    if option_type == OptionType.CALL:
        rho = K * T * exp_neg_rT * norm_cdf(d2) / 100
    else:
        rho = -K * T * exp_neg_rT * norm_cdf(-d2) / 100

    return {
        'delta': delta,
        'gamma': gamma,
        'theta': theta,
        'vega': vega,
        'rho': rho
    }

def implied_volatility(
    market_price: float,
    params: OptionParams,
    option_type: OptionType,
    max_iterations: int = 100,
    tolerance: float = 1e-6
) -> float:
    """
    Calculate implied volatility using Newton-Raphson method

    Given a market price, find sigma such that BS_price(sigma) = market_price
    """
    sigma = 0.3  # Initial guess

    for i in range(max_iterations):
        params.sigma = sigma
        price = black_scholes_price(params, option_type)
        diff = price - market_price

        if abs(diff) < tolerance:
            return sigma

        # Vega = d(price)/d(sigma)
        vega = calculate_greeks(params, option_type)['vega'] * 100

        if abs(vega) < 1e-10:
            break

        sigma = sigma - diff / vega

        # Keep sigma in reasonable bounds
        sigma = max(0.01, min(sigma, 5.0))

    return sigma


# Demo
if __name__ == "__main__":
    # Example: ETH option
    params = OptionParams(
        S=3000,      # Spot price $3000
        K=3200,      # Strike $3200 (OTM call)
        T=30/365,    # 30 days to expiry
        r=0.05,      # 5% risk-free rate
        sigma=0.80   # 80% annual volatility
    )

    print("=== Option Pricing ===")
    print(f"Spot: ${params.S}, Strike: ${params.K}")
    print(f"Days to expiry: {params.T * 365:.0f}")
    print(f"Volatility: {params.sigma * 100:.0f}%")
    print()

    call_price = black_scholes_price(params, OptionType.CALL)
    put_price = black_scholes_price(params, OptionType.PUT)

    print(f"Call price: ${call_price:.2f}")
    print(f"Put price:  ${put_price:.2f}")
    print()

    # Verify put-call parity: C - P = S - K*e^(-rT)
    parity_lhs = call_price - put_price
    parity_rhs = params.S - params.K * math.exp(-params.r * params.T)
    print(f"Put-Call Parity check:")
    print(f"  C - P = ${parity_lhs:.2f}")
    print(f"  S - K*e^(-rT) = ${parity_rhs:.2f}")
    print(f"  Difference: ${abs(parity_lhs - parity_rhs):.6f}")
    print()

    # Greeks
    print("=== Greeks (Call) ===")
    greeks = calculate_greeks(params, OptionType.CALL)
    print(f"Delta: {greeks['delta']:.4f}")
    print(f"Gamma: {greeks['gamma']:.6f}")
    print(f"Theta: ${greeks['theta']:.2f}/day")
    print(f"Vega:  ${greeks['vega']:.2f} per 1% vol")
    print(f"Rho:   ${greeks['rho']:.2f} per 1% rate")
    print()

    # Implied volatility
    print("=== Implied Volatility ===")
    market_price = 180  # Suppose market is trading at $180
    iv = implied_volatility(market_price, params, OptionType.CALL)
    print(f"Market price: ${market_price}")
    print(f"Implied vol:  {iv * 100:.2f}%")
```

### 4.2 Delta Hedging Simulator

```python
# delta_hedge.py
import random
from options import OptionParams, OptionType, black_scholes_price, calculate_greeks

def simulate_delta_hedge(
    initial_spot: float,
    strike: float,
    days: int,
    volatility: float,
    hedge_frequency: int = 1,  # Rebalance every N days
    num_contracts: int = 100
) -> dict:
    """
    Simulate delta hedging an option position

    Scenario: You SOLD calls, need to hedge
    """

    # Initial setup
    spot = initial_spot
    T_initial = days / 365
    r = 0.05

    params = OptionParams(S=spot, K=strike, T=T_initial, r=r, sigma=volatility)

    # Sell calls, receive premium
    initial_price = black_scholes_price(params, OptionType.CALL)
    premium_received = initial_price * num_contracts

    print(f"=== Delta Hedging Simulation ===")
    print(f"Sold {num_contracts} calls @ ${initial_price:.2f} = ${premium_received:.2f}")

    # Initial hedge: buy delta shares
    delta = calculate_greeks(params, OptionType.CALL)['delta']
    shares_held = delta * num_contracts
    cash = premium_received - shares_held * spot

    print(f"Initial delta: {delta:.4f}, bought {shares_held:.2f} shares @ ${spot:.2f}")

    history = []

    for day in range(1, days + 1):
        # Simulate price move (geometric Brownian motion)
        daily_vol = volatility / math.sqrt(252)
        daily_return = random.gauss(0, daily_vol)
        spot = spot * math.exp(daily_return)

        # Update time to expiry
        T_remaining = (days - day) / 365

        if T_remaining <= 0:
            # Expiry
            break

        # Rebalance hedge
        if day % hedge_frequency == 0:
            params = OptionParams(S=spot, K=strike, T=T_remaining, r=r, sigma=volatility)
            new_delta = calculate_greeks(params, OptionType.CALL)['delta']
            new_shares = new_delta * num_contracts

            shares_to_trade = new_shares - shares_held
            cash -= shares_to_trade * spot
            shares_held = new_shares

            option_value = black_scholes_price(params, OptionType.CALL) * num_contracts
            portfolio_value = cash + shares_held * spot
            hedge_pnl = portfolio_value - premium_received

            history.append({
                'day': day,
                'spot': spot,
                'delta': new_delta,
                'shares': shares_held,
                'cash': cash,
                'option_value': option_value,
                'hedge_pnl': hedge_pnl
            })

    # Final settlement
    if spot > strike:
        # Call exercised, deliver shares at strike
        final_pnl = cash + shares_held * spot - (spot - strike) * num_contracts
    else:
        # Call expires worthless
        final_pnl = cash + shares_held * spot

    final_pnl -= premium_received  # Account for initial premium

    print(f"\n=== Final Results ===")
    print(f"Final spot: ${spot:.2f}")
    print(f"Strike: ${strike:.2f}")
    print(f"Option {'ITM' if spot > strike else 'OTM'}")
    print(f"Final PnL: ${final_pnl:.2f}")

    return {
        'history': history,
        'final_pnl': final_pnl,
        'final_spot': spot
    }


import math

if __name__ == "__main__":
    # Run simulation
    result = simulate_delta_hedge(
        initial_spot=100,
        strike=105,
        days=30,
        volatility=0.30,
        hedge_frequency=1
    )
```

### 4.3 Exercises

1. Build a volatility surface from market data
2. Implement binomial tree pricing
3. Build a Monte Carlo pricer for exotic options
4. Create a gamma scalping simulator
5. Build an options market maker

---

## Project 5: Funding Rate Arbitrage Bot (Week 5-6)

### 5.1 The Strategy

```python
# funding_arb.py
"""
Funding Rate Arbitrage:

When perp trades at premium (perp > spot):
  1. Short perp
  2. Long spot
  3. Collect funding from longs

This is delta-neutral and earns the funding rate.
"""

from dataclasses import dataclass
from typing import Optional
import time

@dataclass
class ArbitragePosition:
    spot_size: float       # Positive = long spot
    perp_size: float       # Negative = short perp
    spot_entry: float
    perp_entry: float
    margin_used: float
    funding_collected: float
    fees_paid: float
    opened_at: float

class FundingArbitrageBot:
    def __init__(
        self,
        capital: float,
        max_leverage: float = 2.0,
        min_funding_rate: float = 0.0005,  # 0.05% minimum to enter
        maker_fee: float = 0.0002,         # 0.02%
        taker_fee: float = 0.0005,         # 0.05%
    ):
        self.capital = capital
        self.max_leverage = max_leverage
        self.min_funding_rate = min_funding_rate
        self.maker_fee = maker_fee
        self.taker_fee = taker_fee

        self.position: Optional[ArbitragePosition] = None
        self.pnl_history = []

    def check_opportunity(
        self,
        spot_price: float,
        perp_price: float,
        funding_rate: float,
        funding_interval_hours: float = 8
    ) -> dict:
        """Analyze if arbitrage is profitable"""

        # Calculate annualized funding rate
        funding_per_year = funding_rate * (365 * 24 / funding_interval_hours)

        # Calculate basis (premium/discount)
        basis = (perp_price - spot_price) / spot_price

        # Calculate entry costs (fees for both legs)
        entry_cost = self.taker_fee * 2  # Buy spot + short perp
        exit_cost = self.taker_fee * 2   # Sell spot + close perp
        total_cost = entry_cost + exit_cost

        # Expected profit per funding period
        expected_profit = abs(funding_rate) - total_cost

        is_profitable = (
            abs(funding_rate) >= self.min_funding_rate and
            expected_profit > 0 and
            # Only enter when perp is at premium and funding is positive
            # (or perp at discount and funding negative)
            (basis > 0 and funding_rate > 0) or
            (basis < 0 and funding_rate < 0)
        )

        return {
            'basis': basis,
            'basis_bps': basis * 10000,
            'funding_rate': funding_rate,
            'funding_annualized': funding_per_year,
            'entry_cost': entry_cost,
            'expected_profit_per_period': expected_profit,
            'is_profitable': is_profitable,
            'direction': 'short_perp' if funding_rate > 0 else 'long_perp'
        }

    def open_position(
        self,
        spot_price: float,
        perp_price: float,
        size: float
    ):
        """Open delta-neutral position"""

        if self.position is not None:
            print("Already have position")
            return

        notional = size * spot_price
        margin_needed = notional / self.max_leverage

        if margin_needed > self.capital:
            print(f"Insufficient capital. Need ${margin_needed:.2f}")
            return

        # Fees
        spot_fee = size * spot_price * self.taker_fee
        perp_fee = size * perp_price * self.taker_fee
        total_fees = spot_fee + perp_fee

        self.position = ArbitragePosition(
            spot_size=size,
            perp_size=-size,  # Short perp
            spot_entry=spot_price,
            perp_entry=perp_price,
            margin_used=margin_needed,
            funding_collected=0,
            fees_paid=total_fees,
            opened_at=time.time()
        )

        self.capital -= margin_needed + total_fees

        print(f"Opened arb position:")
        print(f"  Long {size:.4f} spot @ ${spot_price:.2f}")
        print(f"  Short {size:.4f} perp @ ${perp_price:.2f}")
        print(f"  Margin: ${margin_needed:.2f}")
        print(f"  Fees: ${total_fees:.2f}")

    def collect_funding(self, funding_rate: float, mark_price: float):
        """Collect funding payment (called every funding interval)"""

        if self.position is None:
            return

        # Funding payment = position size * mark price * funding rate
        # Short position receives positive funding
        payment = abs(self.position.perp_size) * mark_price * funding_rate

        if self.position.perp_size < 0 and funding_rate > 0:
            # We're short, rate is positive -> we receive
            self.position.funding_collected += payment
            self.capital += payment
            print(f"Received funding: ${payment:.2f}")
        elif self.position.perp_size < 0 and funding_rate < 0:
            # We're short, rate is negative -> we pay
            self.position.funding_collected -= abs(payment)
            self.capital -= abs(payment)
            print(f"Paid funding: ${abs(payment):.2f}")

    def close_position(self, spot_price: float, perp_price: float) -> float:
        """Close position and calculate PnL"""

        if self.position is None:
            return 0

        pos = self.position

        # PnL from spot leg
        spot_pnl = (spot_price - pos.spot_entry) * pos.spot_size

        # PnL from perp leg (we're short)
        perp_pnl = (pos.perp_entry - perp_price) * abs(pos.perp_size)

        # Exit fees
        exit_fees = (pos.spot_size * spot_price + abs(pos.perp_size) * perp_price) * self.taker_fee

        # Total PnL
        total_pnl = (
            spot_pnl +
            perp_pnl +
            pos.funding_collected -
            pos.fees_paid -
            exit_fees
        )

        # Return capital
        self.capital += pos.margin_used + total_pnl

        print(f"Closed position:")
        print(f"  Spot PnL: ${spot_pnl:.2f}")
        print(f"  Perp PnL: ${perp_pnl:.2f}")
        print(f"  Funding: ${pos.funding_collected:.2f}")
        print(f"  Fees: ${pos.fees_paid + exit_fees:.2f}")
        print(f"  Net PnL: ${total_pnl:.2f}")

        self.pnl_history.append(total_pnl)
        self.position = None

        return total_pnl

    def get_stats(self) -> dict:
        return {
            'capital': self.capital,
            'total_pnl': sum(self.pnl_history),
            'num_trades': len(self.pnl_history),
            'avg_pnl': sum(self.pnl_history) / len(self.pnl_history) if self.pnl_history else 0,
            'has_position': self.position is not None
        }


# Simulation
def simulate_funding_arb():
    bot = FundingArbitrageBot(capital=10000)

    # Simulate market data
    spot = 50000
    perp = 50250  # 0.5% premium
    funding_rate = 0.001  # 0.1% per 8h

    print("=== Funding Arb Analysis ===")
    analysis = bot.check_opportunity(spot, perp, funding_rate)
    for k, v in analysis.items():
        print(f"  {k}: {v}")

    if analysis['is_profitable']:
        # Open position (1 BTC notional)
        size = 1.0
        bot.open_position(spot, perp, size)

        # Simulate 7 days (21 funding periods)
        print("\n=== Simulating 7 days ===")
        for period in range(21):
            # Small random price movements
            import random
            spot *= 1 + random.gauss(0, 0.01)
            perp = spot * (1 + random.gauss(0.003, 0.002))  # Maintains small premium

            # Collect funding
            bot.collect_funding(0.001, perp)

        # Close position
        print("\n=== Closing Position ===")
        bot.close_position(spot, perp)

    print(f"\n=== Final Stats ===")
    for k, v in bot.get_stats().items():
        print(f"  {k}: {v}")


if __name__ == "__main__":
    simulate_funding_arb()
```

### 5.2 Exercises

1. Add real exchange API integration (Binance, Bybit)
2. Implement real-time funding rate monitoring
3. Add position sizing based on Kelly criterion
4. Build risk limits (max position, max drawdown)
5. Add execution optimization (reduce slippage)

---

## Project 6: Build a DEX Perps Protocol (Week 6-8)

This is the capstone. Build a simplified on-chain perps exchange.

### 6.1 Solidity Implementation

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

/**
 * @title SimplePerpetuals
 * @notice A minimal perpetual futures implementation for learning
 */
contract SimplePerpetuals {
    using SafeERC20 for IERC20;

    // Collateral token (e.g., USDC)
    IERC20 public immutable collateral;

    // Oracle for index price
    address public oracle;

    // Position struct
    struct Position {
        int256 size;          // Positive = long, negative = short
        uint256 entryPrice;
        uint256 margin;
        uint256 lastFundingIndex;
    }

    // Market state
    uint256 public indexPrice;
    uint256 public markPrice;
    int256 public fundingRate;        // Per-second rate (scaled by 1e18)
    uint256 public lastFundingUpdate;
    uint256 public cumulativeFundingIndex;  // Accumulated funding

    // Open interest
    uint256 public openInterestLong;
    uint256 public openInterestShort;

    // Positions by user
    mapping(address => Position) public positions;

    // Parameters (scaled by 1e18)
    uint256 public constant PRECISION = 1e18;
    uint256 public maintenanceMarginRatio = 5e16;  // 5%
    uint256 public maxLeverage = 20;
    uint256 public fundingRateMax = 1e14;  // ~0.01% per 8h

    // Insurance fund
    uint256 public insuranceFund;

    // Events
    event PositionOpened(address indexed user, int256 size, uint256 price, uint256 margin);
    event PositionClosed(address indexed user, int256 pnl);
    event Liquidation(address indexed user, address indexed liquidator, uint256 reward);
    event FundingUpdated(int256 rate, uint256 cumulativeIndex);

    constructor(address _collateral, address _oracle) {
        collateral = IERC20(_collateral);
        oracle = _oracle;
    }

    /**
     * @notice Open a new position
     * @param size Position size (positive = long, negative = short)
     * @param margin Collateral amount
     */
    function openPosition(int256 size, uint256 margin) external {
        require(positions[msg.sender].size == 0, "Position exists");
        require(size != 0, "Size cannot be zero");

        // Check leverage
        uint256 notional = abs(size) * markPrice / PRECISION;
        require(notional / margin <= maxLeverage, "Exceeds max leverage");

        // Update funding before any position change
        _updateFunding();

        // Transfer collateral
        collateral.safeTransferFrom(msg.sender, address(this), margin);

        // Create position
        positions[msg.sender] = Position({
            size: size,
            entryPrice: markPrice,
            margin: margin,
            lastFundingIndex: cumulativeFundingIndex
        });

        // Update open interest
        if (size > 0) {
            openInterestLong += uint256(size);
        } else {
            openInterestShort += uint256(-size);
        }

        emit PositionOpened(msg.sender, size, markPrice, margin);
    }

    /**
     * @notice Close position
     */
    function closePosition() external {
        Position storage pos = positions[msg.sender];
        require(pos.size != 0, "No position");

        _updateFunding();

        // Calculate PnL
        int256 pnl = _calculatePnL(msg.sender);
        int256 fundingPayment = _calculateFundingPayment(msg.sender);

        int256 totalPnL = pnl - fundingPayment;
        uint256 toReturn;

        if (totalPnL >= 0) {
            toReturn = pos.margin + uint256(totalPnL);
        } else {
            if (uint256(-totalPnL) > pos.margin) {
                // Bankrupt - take from insurance fund
                toReturn = 0;
            } else {
                toReturn = pos.margin - uint256(-totalPnL);
            }
        }

        // Update open interest
        if (pos.size > 0) {
            openInterestLong -= uint256(pos.size);
        } else {
            openInterestShort -= uint256(-pos.size);
        }

        // Clear position
        delete positions[msg.sender];

        // Return collateral
        if (toReturn > 0) {
            collateral.safeTransfer(msg.sender, toReturn);
        }

        emit PositionClosed(msg.sender, totalPnL);
    }

    /**
     * @notice Liquidate undercollateralized position
     */
    function liquidate(address user) external {
        Position storage pos = positions[user];
        require(pos.size != 0, "No position");

        _updateFunding();

        // Check if liquidatable
        require(_isLiquidatable(user), "Not liquidatable");

        // Calculate remaining margin after PnL
        int256 pnl = _calculatePnL(user);
        int256 fundingPayment = _calculateFundingPayment(user);
        int256 remainingMargin = int256(pos.margin) + pnl - fundingPayment;

        // Liquidator reward (1% of position)
        uint256 reward = abs(pos.size) * markPrice / PRECISION / 100;

        // Update open interest
        if (pos.size > 0) {
            openInterestLong -= uint256(pos.size);
        } else {
            openInterestShort -= uint256(-pos.size);
        }

        // Clear position
        delete positions[user];

        // Pay liquidator
        if (remainingMargin > int256(reward)) {
            collateral.safeTransfer(msg.sender, reward);
            insuranceFund += uint256(remainingMargin) - reward;
        } else if (remainingMargin > 0) {
            collateral.safeTransfer(msg.sender, uint256(remainingMargin));
        }

        emit Liquidation(user, msg.sender, reward);
    }

    /**
     * @notice Update funding rate based on premium
     */
    function _updateFunding() internal {
        if (block.timestamp == lastFundingUpdate) return;

        uint256 timeDelta = block.timestamp - lastFundingUpdate;

        // Calculate funding rate: (mark - index) / index
        int256 premium = int256(markPrice) - int256(indexPrice);
        int256 rate = (premium * int256(PRECISION)) / int256(indexPrice);

        // Clamp to max
        if (rate > int256(fundingRateMax)) rate = int256(fundingRateMax);
        if (rate < -int256(fundingRateMax)) rate = -int256(fundingRateMax);

        fundingRate = rate;

        // Update cumulative funding
        cumulativeFundingIndex += uint256(abs(rate)) * timeDelta;
        lastFundingUpdate = block.timestamp;

        emit FundingUpdated(rate, cumulativeFundingIndex);
    }

    function _calculatePnL(address user) internal view returns (int256) {
        Position storage pos = positions[user];

        if (pos.size > 0) {
            // Long: profit when price goes up
            return int256(markPrice - pos.entryPrice) * pos.size / int256(PRECISION);
        } else {
            // Short: profit when price goes down
            return int256(pos.entryPrice - markPrice) * (-pos.size) / int256(PRECISION);
        }
    }

    function _calculateFundingPayment(address user) internal view returns (int256) {
        Position storage pos = positions[user];

        uint256 fundingDelta = cumulativeFundingIndex - pos.lastFundingIndex;
        int256 payment = int256(fundingDelta) * pos.size / int256(PRECISION);

        return payment;
    }

    function _isLiquidatable(address user) internal view returns (bool) {
        Position storage pos = positions[user];

        int256 pnl = _calculatePnL(user);
        int256 funding = _calculateFundingPayment(user);
        int256 equity = int256(pos.margin) + pnl - funding;

        uint256 notional = abs(pos.size) * markPrice / PRECISION;
        uint256 maintenanceMargin = notional * maintenanceMarginRatio / PRECISION;

        return equity < int256(maintenanceMargin);
    }

    function abs(int256 x) internal pure returns (uint256) {
        return x >= 0 ? uint256(x) : uint256(-x);
    }

    // Oracle functions (simplified)
    function updatePrice(uint256 _indexPrice, uint256 _markPrice) external {
        require(msg.sender == oracle, "Only oracle");
        indexPrice = _indexPrice;
        markPrice = _markPrice;
    }
}
```

### 6.2 Exercises for the Contract

1. Add tests in Foundry
2. Implement partial position close
3. Add ADL (Auto-Deleveraging) mechanism
4. Implement order book or AMM for price discovery
5. Add multi-collateral support
6. Implement cross-margin mode

---

## Project 7: Build an AMM DEX (Week 8-10)

Understand how Uniswap-style DEXes work by building one.

### 7.1 Constant Product AMM (x * y = k)

```python
# amm.py
"""
Constant Product Market Maker (Uniswap V2 style)

The core invariant: x * y = k
Where x and y are reserves of two tokens.
"""

from dataclasses import dataclass
from typing import Tuple

@dataclass
class LiquidityPool:
    reserve_x: float  # Token X reserves
    reserve_y: float  # Token Y reserves
    total_lp_shares: float  # Total LP tokens issued
    fee_rate: float = 0.003  # 0.3% swap fee

    @property
    def k(self) -> float:
        """The invariant"""
        return self.reserve_x * self.reserve_y

    @property
    def price_x_in_y(self) -> float:
        """Price of X in terms of Y"""
        return self.reserve_y / self.reserve_x

    @property
    def price_y_in_x(self) -> float:
        """Price of Y in terms of X"""
        return self.reserve_x / self.reserve_y


class ConstantProductAMM:
    def __init__(self):
        self.pool: LiquidityPool = None
        self.lp_balances: dict[str, float] = {}
        self.fee_collected_x: float = 0
        self.fee_collected_y: float = 0

    def create_pool(
        self,
        creator: str,
        amount_x: float,
        amount_y: float,
        fee_rate: float = 0.003
    ) -> float:
        """
        Create a new liquidity pool
        Initial LP shares = sqrt(x * y)
        """
        import math

        initial_shares = math.sqrt(amount_x * amount_y)

        self.pool = LiquidityPool(
            reserve_x=amount_x,
            reserve_y=amount_y,
            total_lp_shares=initial_shares,
            fee_rate=fee_rate
        )

        self.lp_balances[creator] = initial_shares

        print(f"Pool created: {amount_x} X + {amount_y} Y")
        print(f"Initial price: 1 X = {self.pool.price_x_in_y:.4f} Y")
        print(f"LP shares minted: {initial_shares:.4f}")

        return initial_shares

    def add_liquidity(
        self,
        user: str,
        amount_x: float,
        amount_y: float
    ) -> Tuple[float, float, float]:
        """
        Add liquidity to the pool
        Must add in current ratio to avoid arbitrage
        Returns: (actual_x, actual_y, lp_shares)
        """
        pool = self.pool

        # Calculate the required ratio
        current_ratio = pool.reserve_x / pool.reserve_y
        provided_ratio = amount_x / amount_y

        # Adjust amounts to match pool ratio
        if provided_ratio > current_ratio:
            # Too much X provided, use all Y
            actual_y = amount_y
            actual_x = amount_y * current_ratio
        else:
            # Too much Y provided, use all X
            actual_x = amount_x
            actual_y = amount_x / current_ratio

        # Calculate LP shares: proportional to contribution
        share_ratio = actual_x / pool.reserve_x
        lp_shares = pool.total_lp_shares * share_ratio

        # Update pool
        pool.reserve_x += actual_x
        pool.reserve_y += actual_y
        pool.total_lp_shares += lp_shares

        self.lp_balances[user] = self.lp_balances.get(user, 0) + lp_shares

        print(f"{user} added liquidity: {actual_x:.4f} X + {actual_y:.4f} Y")
        print(f"LP shares received: {lp_shares:.4f}")

        return actual_x, actual_y, lp_shares

    def remove_liquidity(
        self,
        user: str,
        lp_shares: float
    ) -> Tuple[float, float]:
        """
        Remove liquidity by burning LP shares
        Returns: (amount_x, amount_y)
        """
        pool = self.pool

        if self.lp_balances.get(user, 0) < lp_shares:
            raise ValueError("Insufficient LP balance")

        # Calculate share of pool
        share_ratio = lp_shares / pool.total_lp_shares

        amount_x = pool.reserve_x * share_ratio
        amount_y = pool.reserve_y * share_ratio

        # Update pool
        pool.reserve_x -= amount_x
        pool.reserve_y -= amount_y
        pool.total_lp_shares -= lp_shares

        self.lp_balances[user] -= lp_shares

        print(f"{user} removed liquidity: {amount_x:.4f} X + {amount_y:.4f} Y")
        print(f"LP shares burned: {lp_shares:.4f}")

        return amount_x, amount_y

    def swap_x_for_y(self, amount_x_in: float) -> float:
        """
        Swap X tokens for Y tokens

        Math:
        - New reserve_x = reserve_x + amount_in * (1 - fee)
        - k must stay constant: new_x * new_y = k
        - new_y = k / new_x
        - amount_out = reserve_y - new_y
        """
        pool = self.pool

        # Apply fee
        fee = amount_x_in * pool.fee_rate
        amount_x_after_fee = amount_x_in - fee

        # Calculate output using constant product formula
        new_reserve_x = pool.reserve_x + amount_x_after_fee
        new_reserve_y = pool.k / new_reserve_x
        amount_y_out = pool.reserve_y - new_reserve_y

        # Update reserves
        pool.reserve_x = new_reserve_x
        pool.reserve_y = new_reserve_y

        # Track fees (added to X reserves)
        self.fee_collected_x += fee
        pool.reserve_x += fee  # Fees go to LPs

        # Calculate effective price
        effective_price = amount_x_in / amount_y_out
        spot_price_before = (pool.reserve_y + amount_y_out) / (pool.reserve_x - amount_x_in)
        price_impact = (effective_price / spot_price_before - 1) * 100

        print(f"Swap: {amount_x_in:.4f} X -> {amount_y_out:.4f} Y")
        print(f"Effective price: {effective_price:.4f} X per Y")
        print(f"Price impact: {price_impact:.2f}%")

        return amount_y_out

    def swap_y_for_x(self, amount_y_in: float) -> float:
        """Swap Y tokens for X tokens"""
        pool = self.pool

        fee = amount_y_in * pool.fee_rate
        amount_y_after_fee = amount_y_in - fee

        new_reserve_y = pool.reserve_y + amount_y_after_fee
        new_reserve_x = pool.k / new_reserve_y
        amount_x_out = pool.reserve_x - new_reserve_x

        pool.reserve_x = new_reserve_x
        pool.reserve_y = new_reserve_y

        self.fee_collected_y += fee
        pool.reserve_y += fee

        print(f"Swap: {amount_y_in:.4f} Y -> {amount_x_out:.4f} X")

        return amount_x_out

    def get_quote(self, amount_in: float, x_to_y: bool) -> dict:
        """Get quote for a swap without executing"""
        pool = self.pool

        if x_to_y:
            reserve_in, reserve_out = pool.reserve_x, pool.reserve_y
        else:
            reserve_in, reserve_out = pool.reserve_y, pool.reserve_x

        fee = amount_in * pool.fee_rate
        amount_after_fee = amount_in - fee

        new_reserve_in = reserve_in + amount_after_fee
        new_reserve_out = (reserve_in * reserve_out) / new_reserve_in
        amount_out = reserve_out - new_reserve_out

        effective_price = amount_in / amount_out
        spot_price = reserve_in / reserve_out
        price_impact = (effective_price / spot_price - 1) * 100
        slippage = (1 - amount_out / (amount_in / spot_price)) * 100

        return {
            'amount_out': amount_out,
            'effective_price': effective_price,
            'spot_price': spot_price,
            'price_impact': price_impact,
            'slippage': slippage,
            'fee': fee
        }

    def print_status(self):
        pool = self.pool
        print(f"\n=== Pool Status ===")
        print(f"Reserve X: {pool.reserve_x:.4f}")
        print(f"Reserve Y: {pool.reserve_y:.4f}")
        print(f"k = {pool.k:.4f}")
        print(f"Price: 1 X = {pool.price_x_in_y:.4f} Y")
        print(f"Total LP shares: {pool.total_lp_shares:.4f}")
        print(f"Fees collected: {self.fee_collected_x:.4f} X, {self.fee_collected_y:.4f} Y")


# Demo: Simulate DEX trading
if __name__ == "__main__":
    amm = ConstantProductAMM()

    # Create ETH/USDC pool (1 ETH = 3000 USDC)
    amm.create_pool("alice", amount_x=100, amount_y=300000)  # 100 ETH, 300k USDC

    amm.print_status()

    # Simulate trades
    print("\n--- Trade 1: Buy 10 ETH with USDC ---")
    quote = amm.get_quote(35000, x_to_y=False)
    print(f"Quote: {35000} USDC -> {quote['amount_out']:.4f} ETH")
    print(f"Price impact: {quote['price_impact']:.2f}%")

    amm.swap_y_for_x(35000)
    amm.print_status()

    print("\n--- Trade 2: Sell 5 ETH for USDC ---")
    amm.swap_x_for_y(5)
    amm.print_status()

    # Add more liquidity
    print("\n--- Bob adds liquidity ---")
    amm.add_liquidity("bob", 50, 200000)
    amm.print_status()

    # Large trade to show slippage
    print("\n--- Large trade: Buy 30 ETH ---")
    quote = amm.get_quote(150000, x_to_y=False)
    print(f"Quote: 150000 USDC -> {quote['amount_out']:.4f} ETH")
    print(f"Slippage: {quote['slippage']:.2f}%")
```

### 7.2 Understanding Impermanent Loss

```python
# impermanent_loss.py
"""
Impermanent Loss Calculator

IL happens when you LP instead of just holding.
If price changes, you would have been better off holding.
"""

import math

def calculate_impermanent_loss(price_ratio: float) -> float:
    """
    Calculate IL given price change

    price_ratio = new_price / original_price

    IL = 2 * sqrt(price_ratio) / (1 + price_ratio) - 1
    """
    return 2 * math.sqrt(price_ratio) / (1 + price_ratio) - 1


def compare_lp_vs_hold(
    initial_x: float,
    initial_y: float,
    initial_price: float,  # Price of X in Y
    final_price: float
) -> dict:
    """
    Compare LP position vs just holding the tokens
    """

    # Initial values
    initial_value = initial_x * initial_price + initial_y

    # HODL scenario
    hodl_value = initial_x * final_price + initial_y

    # LP scenario
    # In AMM: x * y = k, and price = y/x
    # When price changes: new_x = sqrt(k/new_price), new_y = sqrt(k * new_price)
    k = initial_x * initial_y
    new_x = math.sqrt(k / final_price)
    new_y = math.sqrt(k * final_price)
    lp_value = new_x * final_price + new_y

    # Impermanent loss
    il = (lp_value / hodl_value) - 1

    price_ratio = final_price / initial_price
    theoretical_il = calculate_impermanent_loss(price_ratio)

    return {
        'initial_value': initial_value,
        'hodl_value': hodl_value,
        'lp_value': lp_value,
        'il_percent': il * 100,
        'il_absolute': hodl_value - lp_value,
        'theoretical_il': theoretical_il * 100,
        'price_change': (price_ratio - 1) * 100
    }


# Visualize IL curve
def print_il_table():
    print("=== Impermanent Loss Table ===")
    print("Price Change | IL")
    print("-" * 25)

    price_changes = [0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0, 3.0, 4.0, 5.0]

    for ratio in price_changes:
        il = calculate_impermanent_loss(ratio)
        change_pct = (ratio - 1) * 100
        print(f"{change_pct:+7.0f}%      | {il * 100:6.2f}%")


if __name__ == "__main__":
    print_il_table()

    print("\n=== Example: ETH/USDC LP ===")
    result = compare_lp_vs_hold(
        initial_x=10,       # 10 ETH
        initial_y=30000,    # 30,000 USDC
        initial_price=3000, # 1 ETH = 3000 USDC
        final_price=4500    # ETH pumps 50%
    )

    print(f"Initial: ${result['initial_value']:.2f}")
    print(f"If HODL: ${result['hodl_value']:.2f}")
    print(f"If LP:   ${result['lp_value']:.2f}")
    print(f"IL:      {result['il_percent']:.2f}%")
    print(f"Lost:    ${result['il_absolute']:.2f}")
```

### 7.3 Concentrated Liquidity (Uniswap V3 Style)

```python
# concentrated_liquidity.py
"""
Concentrated Liquidity AMM (Uniswap V3 style)

Instead of spreading liquidity across all prices (0 to ∞),
LPs can concentrate in specific price ranges for higher capital efficiency.
"""

from dataclasses import dataclass
from typing import List, Optional
import math

@dataclass
class LiquidityPosition:
    owner: str
    liquidity: float      # L = sqrt(x * y) for this position
    tick_lower: int       # Lower price bound (as tick)
    tick_upper: int       # Upper price bound (as tick)
    fees_earned_x: float = 0
    fees_earned_y: float = 0

class ConcentratedLiquidityAMM:
    """
    Simplified Uni V3 implementation

    Key concepts:
    - Ticks: discrete price points, price = 1.0001^tick
    - Liquidity is only active within the tick range
    - Capital efficiency = full_range_liquidity / concentrated_liquidity
    """

    def __init__(self, fee_rate: float = 0.003):
        self.positions: List[LiquidityPosition] = []
        self.current_tick: int = 0  # log1.0001(price)
        self.current_price: float = 1.0
        self.fee_rate = fee_rate

        # Global reserves (sum of all active liquidity)
        self.reserve_x: float = 0
        self.reserve_y: float = 0

    def tick_to_price(self, tick: int) -> float:
        """Convert tick to price"""
        return 1.0001 ** tick

    def price_to_tick(self, price: float) -> int:
        """Convert price to nearest tick"""
        return int(math.log(price) / math.log(1.0001))

    def add_liquidity(
        self,
        owner: str,
        amount_x: float,
        amount_y: float,
        price_lower: float,
        price_upper: float
    ) -> LiquidityPosition:
        """
        Add concentrated liquidity between price_lower and price_upper
        """
        tick_lower = self.price_to_tick(price_lower)
        tick_upper = self.price_to_tick(price_upper)

        # Calculate liquidity
        sqrt_price_lower = math.sqrt(price_lower)
        sqrt_price_upper = math.sqrt(price_upper)
        sqrt_price_current = math.sqrt(self.current_price)

        # Liquidity calculation depends on current price vs range
        if self.current_price < price_lower:
            # All in Y (waiting for price to enter range)
            liquidity = amount_y / (sqrt_price_upper - sqrt_price_lower)
        elif self.current_price > price_upper:
            # All in X (price above range)
            liquidity = amount_x / (1/sqrt_price_lower - 1/sqrt_price_upper)
        else:
            # Price in range - need both tokens
            liquidity_from_x = amount_x / (1/sqrt_price_current - 1/sqrt_price_upper)
            liquidity_from_y = amount_y / (sqrt_price_current - sqrt_price_lower)
            liquidity = min(liquidity_from_x, liquidity_from_y)

        position = LiquidityPosition(
            owner=owner,
            liquidity=liquidity,
            tick_lower=tick_lower,
            tick_upper=tick_upper
        )

        self.positions.append(position)

        print(f"Added liquidity: L={liquidity:.2f} in range [{price_lower:.2f}, {price_upper:.2f}]")

        # Capital efficiency vs full range
        full_range_liquidity = math.sqrt(amount_x * amount_y)
        efficiency = liquidity / full_range_liquidity if full_range_liquidity > 0 else 0
        print(f"Capital efficiency: {efficiency:.1f}x vs full range")

        return position

    def get_active_liquidity(self) -> float:
        """Get total liquidity active at current price"""
        total = 0
        for pos in self.positions:
            price_lower = self.tick_to_price(pos.tick_lower)
            price_upper = self.tick_to_price(pos.tick_upper)

            if price_lower <= self.current_price <= price_upper:
                total += pos.liquidity

        return total

    def swap_y_for_x(self, amount_y_in: float) -> float:
        """
        Swap Y for X (buying X, price goes up)

        This is simplified - real V3 crosses ticks as price moves
        """
        active_liquidity = self.get_active_liquidity()

        if active_liquidity == 0:
            print("No liquidity at current price!")
            return 0

        fee = amount_y_in * self.fee_rate
        amount_y_after_fee = amount_y_in - fee

        # Simplified: use xy=L^2 locally
        # In real V3, you'd integrate across tick boundaries
        sqrt_price_old = math.sqrt(self.current_price)
        sqrt_price_new = sqrt_price_old + amount_y_after_fee / active_liquidity

        new_price = sqrt_price_new ** 2

        # Amount of X out
        amount_x_out = active_liquidity * (1/sqrt_price_old - 1/sqrt_price_new)

        old_price = self.current_price
        self.current_price = new_price
        self.current_tick = self.price_to_tick(new_price)

        # Distribute fees to active LPs
        self._distribute_fees(0, fee, old_price)

        print(f"Swap: {amount_y_in:.4f} Y -> {amount_x_out:.4f} X")
        print(f"Price: {old_price:.4f} -> {new_price:.4f}")

        return amount_x_out

    def _distribute_fees(self, fee_x: float, fee_y: float, price: float):
        """Distribute fees to LPs with active positions"""
        active_positions = []
        total_liquidity = 0

        for pos in self.positions:
            price_lower = self.tick_to_price(pos.tick_lower)
            price_upper = self.tick_to_price(pos.tick_upper)

            if price_lower <= price <= price_upper:
                active_positions.append(pos)
                total_liquidity += pos.liquidity

        for pos in active_positions:
            share = pos.liquidity / total_liquidity
            pos.fees_earned_x += fee_x * share
            pos.fees_earned_y += fee_y * share

    def print_status(self):
        print(f"\n=== Concentrated Liquidity Pool ===")
        print(f"Current price: {self.current_price:.4f}")
        print(f"Current tick: {self.current_tick}")
        print(f"Active liquidity: {self.get_active_liquidity():.2f}")
        print(f"\nPositions:")
        for i, pos in enumerate(self.positions):
            price_lower = self.tick_to_price(pos.tick_lower)
            price_upper = self.tick_to_price(pos.tick_upper)
            active = "✓" if price_lower <= self.current_price <= price_upper else " "
            print(f"  [{active}] {pos.owner}: L={pos.liquidity:.2f} "
                  f"[{price_lower:.2f}-{price_upper:.2f}] "
                  f"fees=({pos.fees_earned_x:.4f} X, {pos.fees_earned_y:.4f} Y)")


if __name__ == "__main__":
    amm = ConcentratedLiquidityAMM()
    amm.current_price = 3000  # ETH/USDC
    amm.current_tick = amm.price_to_tick(3000)

    # LP1: Wide range (like V2)
    amm.add_liquidity("alice", 10, 30000, 1000, 10000)

    # LP2: Tight range around current price
    amm.add_liquidity("bob", 10, 30000, 2800, 3200)

    amm.print_status()

    # Trade that moves price
    print("\n--- Trade: Buy ETH with 50000 USDC ---")
    amm.swap_y_for_x(50000)
    amm.print_status()
```

### 7.4 DEX Aggregator / Router

```python
# dex_aggregator.py
"""
DEX Aggregator

Finds the best route across multiple pools/DEXes
to minimize slippage and maximize output.
"""

from dataclasses import dataclass
from typing import List, Dict, Tuple
import heapq

@dataclass
class Pool:
    name: str
    token_a: str
    token_b: str
    reserve_a: float
    reserve_b: float
    fee_rate: float

    def get_quote(self, token_in: str, amount_in: float) -> Tuple[str, float]:
        """Get output amount for a swap"""
        if token_in == self.token_a:
            reserve_in, reserve_out = self.reserve_a, self.reserve_b
            token_out = self.token_b
        else:
            reserve_in, reserve_out = self.reserve_b, self.reserve_a
            token_out = self.token_a

        amount_after_fee = amount_in * (1 - self.fee_rate)
        amount_out = (reserve_out * amount_after_fee) / (reserve_in + amount_after_fee)

        return token_out, amount_out


class DEXAggregator:
    def __init__(self):
        self.pools: List[Pool] = []
        self.pool_map: Dict[Tuple[str, str], List[Pool]] = {}

    def add_pool(self, pool: Pool):
        self.pools.append(pool)

        # Index by token pair (both directions)
        key1 = (pool.token_a, pool.token_b)
        key2 = (pool.token_b, pool.token_a)

        if key1 not in self.pool_map:
            self.pool_map[key1] = []
        if key2 not in self.pool_map:
            self.pool_map[key2] = []

        self.pool_map[key1].append(pool)
        self.pool_map[key2].append(pool)

    def find_best_direct(
        self,
        token_in: str,
        token_out: str,
        amount_in: float
    ) -> Tuple[Pool, float]:
        """Find best single-hop swap"""
        key = (token_in, token_out)

        if key not in self.pool_map:
            return None, 0

        best_pool = None
        best_output = 0

        for pool in self.pool_map[key]:
            _, output = pool.get_quote(token_in, amount_in)
            if output > best_output:
                best_output = output
                best_pool = pool

        return best_pool, best_output

    def find_best_route(
        self,
        token_in: str,
        token_out: str,
        amount_in: float,
        max_hops: int = 3
    ) -> Tuple[List[Pool], float]:
        """
        Find best multi-hop route using BFS

        This is simplified - real aggregators use more sophisticated
        algorithms (split routes, parallel paths, etc.)
        """
        # Get all unique tokens
        tokens = set()
        for pool in self.pools:
            tokens.add(pool.token_a)
            tokens.add(pool.token_b)

        # BFS: (negative_output, hops, current_token, route, amount)
        # Negative because heapq is min-heap
        queue = [(-amount_in, 0, token_in, [], amount_in)]

        best_output = 0
        best_route = []

        visited_states = set()

        while queue:
            neg_amount, hops, current_token, route, current_amount = heapq.heappop(queue)

            # Check if we've reached destination
            if current_token == token_out and current_amount > best_output:
                best_output = current_amount
                best_route = route
                continue

            if hops >= max_hops:
                continue

            # Explore next hops
            for pool in self.pools:
                if pool in route:
                    continue

                next_token = None
                if pool.token_a == current_token:
                    next_token = pool.token_b
                elif pool.token_b == current_token:
                    next_token = pool.token_a

                if next_token is None:
                    continue

                _, output = pool.get_quote(current_token, current_amount)

                if output <= 0:
                    continue

                state = (next_token, tuple(route + [pool.name]))
                if state in visited_states:
                    continue
                visited_states.add(state)

                heapq.heappush(queue, (
                    -output,
                    hops + 1,
                    next_token,
                    route + [pool],
                    output
                ))

        return best_route, best_output

    def split_route(
        self,
        token_in: str,
        token_out: str,
        amount_in: float,
        num_splits: int = 4
    ) -> Dict:
        """
        Split trade across multiple pools to reduce slippage

        Simple greedy approach: split into equal parts and route each
        """
        split_amount = amount_in / num_splits
        total_output = 0
        routes = []

        # Clone pools for simulation
        for i in range(num_splits):
            route, output = self.find_best_route(token_in, token_out, split_amount)
            total_output += output
            routes.append({
                'amount_in': split_amount,
                'amount_out': output,
                'route': [p.name for p in route]
            })

        # Compare to single route
        single_route, single_output = self.find_best_route(token_in, token_out, amount_in)

        return {
            'split_output': total_output,
            'single_output': single_output,
            'improvement': (total_output / single_output - 1) * 100 if single_output > 0 else 0,
            'routes': routes
        }


# Demo
if __name__ == "__main__":
    agg = DEXAggregator()

    # Add pools (simulating multiple DEXes)
    agg.add_pool(Pool("Uniswap ETH/USDC", "ETH", "USDC", 1000, 3000000, 0.003))
    agg.add_pool(Pool("Sushiswap ETH/USDC", "ETH", "USDC", 800, 2400000, 0.003))
    agg.add_pool(Pool("Curve ETH/USDC", "ETH", "USDC", 2000, 6000000, 0.0004))

    agg.add_pool(Pool("Uniswap ETH/WBTC", "ETH", "WBTC", 500, 30, 0.003))
    agg.add_pool(Pool("Uniswap WBTC/USDC", "WBTC", "USDC", 50, 2500000, 0.003))

    # Find best direct route
    print("=== Direct Route: 10 ETH -> USDC ===")
    pool, output = agg.find_best_direct("ETH", "USDC", 10)
    print(f"Best pool: {pool.name}")
    print(f"Output: {output:.2f} USDC")

    # Find best multi-hop route
    print("\n=== Best Route (multi-hop): 10 ETH -> USDC ===")
    route, output = agg.find_best_route("ETH", "USDC", 10)
    print(f"Route: {' -> '.join([p.name for p in route])}")
    print(f"Output: {output:.2f} USDC")

    # Large trade with split
    print("\n=== Split Route: 100 ETH -> USDC ===")
    result = agg.split_route("ETH", "USDC", 100)
    print(f"Single route output: {result['single_output']:.2f} USDC")
    print(f"Split route output: {result['split_output']:.2f} USDC")
    print(f"Improvement: {result['improvement']:.2f}%")
```

### 7.5 Build an Arbitrage Bot

```python
# dex_arb.py
"""
DEX Arbitrage Bot

Finds and executes arbitrage opportunities across pools.
"""

from typing import List, Tuple, Optional
from dataclasses import dataclass

@dataclass
class ArbOpportunity:
    path: List[str]          # Token path
    pools: List[str]         # Pool names
    input_amount: float
    output_amount: float
    profit: float
    profit_percent: float


class DEXArbitrageBot:
    def __init__(self, pools: List):
        self.pools = {p.name: p for p in pools}
        self.tokens = set()
        for p in pools:
            self.tokens.add(p.token_a)
            self.tokens.add(p.token_b)

    def find_triangular_arb(
        self,
        start_token: str,
        amount: float
    ) -> List[ArbOpportunity]:
        """
        Find triangular arbitrage opportunities
        A -> B -> C -> A
        """
        opportunities = []

        for token_b in self.tokens:
            if token_b == start_token:
                continue

            for token_c in self.tokens:
                if token_c == start_token or token_c == token_b:
                    continue

                # Try path: start -> B -> C -> start
                result = self._simulate_path(
                    [start_token, token_b, token_c, start_token],
                    amount
                )

                if result and result['profit'] > 0:
                    opportunities.append(ArbOpportunity(
                        path=result['path'],
                        pools=result['pools'],
                        input_amount=amount,
                        output_amount=result['output'],
                        profit=result['profit'],
                        profit_percent=result['profit'] / amount * 100
                    ))

        # Sort by profit
        opportunities.sort(key=lambda x: x.profit, reverse=True)
        return opportunities

    def _simulate_path(
        self,
        tokens: List[str],
        amount: float
    ) -> Optional[dict]:
        """Simulate a swap path"""
        current_amount = amount
        pools_used = []

        for i in range(len(tokens) - 1):
            token_in = tokens[i]
            token_out = tokens[i + 1]

            # Find best pool for this hop
            best_pool = None
            best_output = 0

            for pool in self.pools.values():
                if (pool.token_a == token_in and pool.token_b == token_out) or \
                   (pool.token_b == token_in and pool.token_a == token_out):
                    _, output = pool.get_quote(token_in, current_amount)
                    if output > best_output:
                        best_output = output
                        best_pool = pool

            if best_pool is None:
                return None

            pools_used.append(best_pool.name)
            current_amount = best_output

        return {
            'path': tokens,
            'pools': pools_used,
            'output': current_amount,
            'profit': current_amount - amount
        }

    def find_cross_dex_arb(
        self,
        token_a: str,
        token_b: str,
        amount: float
    ) -> Optional[ArbOpportunity]:
        """
        Find arbitrage between same pair on different DEXes
        Buy on cheaper DEX, sell on more expensive one
        """
        pools_for_pair = []

        for pool in self.pools.values():
            if (pool.token_a == token_a and pool.token_b == token_b) or \
               (pool.token_b == token_a and pool.token_a == token_b):
                pools_for_pair.append(pool)

        if len(pools_for_pair) < 2:
            return None

        best_profit = 0
        best_arb = None

        for buy_pool in pools_for_pair:
            for sell_pool in pools_for_pair:
                if buy_pool == sell_pool:
                    continue

                # Buy token_b on buy_pool
                _, amount_b = buy_pool.get_quote(token_a, amount)

                # Sell token_b on sell_pool
                _, amount_a_back = sell_pool.get_quote(token_b, amount_b)

                profit = amount_a_back - amount

                if profit > best_profit:
                    best_profit = profit
                    best_arb = ArbOpportunity(
                        path=[token_a, token_b, token_a],
                        pools=[buy_pool.name, sell_pool.name],
                        input_amount=amount,
                        output_amount=amount_a_back,
                        profit=profit,
                        profit_percent=profit / amount * 100
                    )

        return best_arb


# Demo
if __name__ == "__main__":
    from dex_aggregator import Pool

    # Create pools with slight price discrepancies
    pools = [
        Pool("Uni ETH/USDC", "ETH", "USDC", 1000, 3000000, 0.003),
        Pool("Sushi ETH/USDC", "ETH", "USDC", 1000, 3050000, 0.003),  # ETH slightly cheaper
        Pool("Uni ETH/WBTC", "ETH", "WBTC", 500, 30, 0.003),
        Pool("Uni WBTC/USDC", "WBTC", "USDC", 50, 2400000, 0.003),  # WBTC cheap in USDC
        Pool("Sushi WBTC/USDC", "WBTC", "USDC", 50, 2600000, 0.003), # WBTC expensive
    ]

    bot = DEXArbitrageBot(pools)

    print("=== Cross-DEX Arbitrage: ETH/USDC ===")
    arb = bot.find_cross_dex_arb("USDC", "ETH", 100000)
    if arb:
        print(f"Path: {' -> '.join(arb.path)}")
        print(f"Pools: {' -> '.join(arb.pools)}")
        print(f"Input: {arb.input_amount:.2f} USDC")
        print(f"Output: {arb.output_amount:.2f} USDC")
        print(f"Profit: {arb.profit:.2f} ({arb.profit_percent:.3f}%)")
    else:
        print("No arbitrage found")

    print("\n=== Triangular Arbitrage ===")
    opps = bot.find_triangular_arb("USDC", 100000)
    for opp in opps[:3]:
        print(f"Path: {' -> '.join(opp.path)}")
        print(f"Profit: {opp.profit:.2f} ({opp.profit_percent:.3f}%)")
        print()
```

### 7.6 Exercises for DEX Projects

1. **Flash Loan Arbitrage**: Implement flash loans for capital-free arb
2. **MEV Simulation**: Add mempool simulation and front-running
3. **Sandwich Attack**: Build a sandwich attack simulator (for defense research!)
4. **JIT Liquidity**: Implement just-in-time liquidity provision
5. **TWAP Oracle**: Build a manipulation-resistant price oracle
6. **LP Optimizer**: Auto-rebalance LP positions based on IL
7. **Solidity DEX**: Port the AMM to Solidity with full tests

---

## Learning Path Summary

| Week | Project | What You Build | What You Learn |
|------|---------|---------------|----------------|
| 1 | Order Book | Matching engine | Price-time priority, spreads |
| 2 | Market Maker | Quoting bot | Inventory risk, adverse selection |
| 3-4 | Perp Exchange | Full exchange | Funding, liquidations, margin |
| 4-5 | Options | BS pricer + Greeks | Derivatives math, hedging |
| 5-6 | Funding Arb | Trading bot | Delta-neutral strategies |
| 6-8 | DEX Perps | Solidity protocol | On-chain mechanics |
| 8-10 | AMM DEX | Uniswap clone | x*y=k, IL, concentrated liquidity |
| 10+ | Arb Bots | Multi-DEX router | MEV, arbitrage, aggregation |

---

## Key Principle

> **Build first, understand second.**
>
> When you're stuck on "why does Black-Scholes have these terms?", implement binomial trees first. The continuous-time formula will suddenly make sense.
>
> When you wonder "why do exchanges use funding rates?", build a perp without it and watch prices diverge. Then add funding and see convergence.

Every concept becomes obvious once you've coded it.

---

## Resources for Building

### APIs to Practice With
- **Binance Testnet**: Free paper trading with real market data
- **dYdX Testnet**: DeFi perps testing
- **Bybit Testnet**: Perp trading simulation

### Data Sources
- **CoinGecko API**: Free price data
- **Binance WebSocket**: Real-time order book
- **Kaiko**: Historical data (paid)

### Frameworks
- **CCXT**: Unified exchange API (Python/JS)
- **Foundry**: Solidity testing
- **Hardhat**: Solidity development

Start with Project 1 today. Don't read more theory until you've built the order book.
