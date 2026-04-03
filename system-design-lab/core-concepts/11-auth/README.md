# Authentication & Authorization

## Overview

**Authentication** = "Who are you?" (prove your identity)
**Authorization** = "What can you do?" (check permissions)

These are different questions that use different mechanisms. Every system design interview involves both.

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                  │
│   Authentication (AuthN)          Authorization (AuthZ)          │
│   "Who are you?"                  "What can you do?"             │
│                                                                  │
│   ├── Password                    ├── RBAC (Role-Based)          │
│   ├── OAuth 2.0 / SSO            ├── ABAC (Attribute-Based)     │
│   ├── JWT tokens                  ├── ACL (Access Control List)  │
│   ├── API keys                    └── Policy engines (OPA)       │
│   ├── mTLS (service-to-service)                                  │
│   └── MFA (multi-factor)                                         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## What You Must Master

## 1. JWT (JSON Web Tokens)

The most common token format in modern systems. Stateless — the server doesn't store session state.

```
┌─────────────────────────────────────────────────────────────────┐
│                        JWT Structure                             │
│                                                                  │
│   eyJhbGciOiJIUzI1NiJ9.eyJ1c2VyX2lkIjo0Mn0.abc123signature    │
│   ────────────────────  ─────────────────────  ──────────────   │
│       Header               Payload               Signature      │
│   {"alg":"HS256"}      {"user_id":42,          HMAC-SHA256(     │
│                         "role":"admin",           header+payload │
│                         "exp":1700000000}         secret_key)    │
│                                                                  │
│   Header + Payload = base64 encoded (NOT encrypted!)             │
│   Signature = proves the token wasn't tampered with              │
└─────────────────────────────────────────────────────────────────┘
```

### How JWT Auth Works

```
1. Login:
   Client ──POST /login {email, password}──► Auth Service
   Auth Service ──verifies password──► generates JWT
   Auth Service ──returns JWT──► Client stores it

2. Every subsequent request:
   Client ──GET /api/orders  Authorization: Bearer eyJhb...──► API Gateway
   Gateway ──verify JWT signature (no DB lookup!)──► forward to service

3. Token expiry:
   JWT has exp claim → after expiry, client must refresh or re-login
```

### JWT vs Session Tokens

| | JWT (Stateless) | Session Token (Stateful) |
|---|---|---|
| **Storage** | Client-side (cookie/localStorage) | Server-side (Redis/DB) |
| **Validation** | Verify signature (no DB call) | Lookup in session store |
| **Revocation** | Hard (token valid until expiry) | Easy (delete from store) |
| **Scalability** | Excellent (any server can verify) | Needs shared session store |
| **Size** | Larger (~1KB) | Small (random string) |
| **Best for** | Microservices, APIs | Traditional web apps |

### JWT Gotchas for Interviews

- **JWTs are NOT encrypted** — anyone can decode the payload (base64). Don't put secrets in them.
- **Revocation is hard** — once issued, a JWT is valid until it expires. Solutions: short TTL + refresh tokens, or a token blocklist (defeats the stateless purpose).
- **Token size** — JWTs are bigger than session IDs. With many claims, they can bloat cookies.

## 2. OAuth 2.0

The standard protocol for **delegated authorization** — "let this app access my data without giving it my password."

```
┌─────────────────────────────────────────────────────────────────┐
│                    OAuth 2.0 Flow                                │
│                                                                  │
│   1. User clicks "Login with Google"                             │
│      App ──redirect──► Google login page                         │
│                                                                  │
│   2. User logs in with Google credentials                        │
│      Google ──auth code──► App (redirect back)                   │
│                                                                  │
│   3. App exchanges code for tokens                               │
│      App ──auth code + client secret──► Google                   │
│      Google ──access token + refresh token──► App                │
│                                                                  │
│   4. App uses access token to call Google APIs                   │
│      App ──access token──► Google API (get user profile)         │
│                                                                  │
│   Key: User's password NEVER touches the App                     │
└─────────────────────────────────────────────────────────────────┘
```

### OAuth 2.0 Roles

| Role | Who | Example |
|------|-----|---------|
| **Resource Owner** | The user | You |
| **Client** | The app requesting access | A mobile app |
| **Authorization Server** | Issues tokens | Google, Auth0, Okta |
| **Resource Server** | Hosts the protected data | Google Calendar API |

### OAuth vs OpenID Connect (OIDC)

```
OAuth 2.0 = Authorization ("can this app read my calendar?")
OIDC      = Authentication ("who is this user?") — built ON TOP of OAuth

OIDC adds an ID token (JWT) with user identity claims (name, email, etc.)
```

## 3. API Key Authentication

Simplest auth for service-to-service or developer APIs.

```
Client ──GET /api/data  X-Api-Key: sk-abc123──► Server
Server ──lookup key in DB──► find associated account + permissions

Pros: simple, no login flow
Cons: no user identity, hard to rotate, often leaked in code
```

### API Key Best Practices

- Prefix keys by type: `pk-xxx` (public), `sk-xxx` (secret)
- Hash keys in DB (don't store plaintext)
- Rate limit per key
- Support key rotation (multiple active keys per account)

## 4. Service-to-Service Auth (mTLS)

In microservices, services need to authenticate each other.

```
Without mTLS:
  Service A ──HTTP──► Service B (anyone can call B!)

With mTLS:
  Service A ──presents certificate──► Service B
  Service B ──verifies A's certificate──► "yes, you're really Service A"
  Service B ──presents certificate──► Service A
  BOTH sides verify each other. Encrypted + authenticated.

Managed by: Istio service mesh, Consul Connect, Linkerd
```

## 5. Authorization Models

### RBAC (Role-Based Access Control)

```
Users → Roles → Permissions

  Alice ──► admin  ──► [read, write, delete]
  Bob   ──► editor ──► [read, write]
  Eve   ──► viewer ──► [read]

Query: "Can Bob delete posts?"
  Bob → editor → permissions include delete? NO → denied.

Simple, covers 80% of use cases.
```

### ABAC (Attribute-Based Access Control)

```
Policy: "Allow if user.department == resource.department AND user.clearance >= resource.level"

  Request: {user: {name: "Alice", dept: "engineering", clearance: 3},
            action: "read",
            resource: {type: "doc", dept: "engineering", level: 2}}
  → dept match? YES. clearance 3 >= 2? YES. → ALLOWED.

More flexible than RBAC, but harder to audit and manage.
```

### Permission Checking in Practice

```sql
-- RBAC lookup: "Can user 42 perform 'delete' on 'posts'?"
SELECT 1 FROM user_roles ur
JOIN role_permissions rp ON ur.role_id = rp.role_id
JOIN permissions p ON rp.permission_id = p.id
WHERE ur.user_id = 42
  AND p.resource = 'posts'
  AND p.action = 'delete';

-- If row found → allowed. Otherwise → 403 Forbidden.
```

## 6. Token Refresh Flow

```
┌─────────────────────────────────────────────────────────────────┐
│   Access Token:  short-lived (15 min), used for every request   │
│   Refresh Token: long-lived (7 days), used to get new access    │
│                                                                  │
│   1. Login → get both tokens                                     │
│   2. Use access token for API calls                              │
│   3. Access token expires (15 min)                               │
│   4. Use refresh token to get new access token                   │
│   5. Refresh token expires (7 days) → must re-login              │
│                                                                  │
│   Why two tokens?                                                │
│   - Access token is sent with EVERY request (high exposure)      │
│   - If stolen, attacker gets only 15 min window                  │
│   - Refresh token is sent rarely (low exposure)                  │
│   - Refresh token can be revoked server-side                     │
└─────────────────────────────────────────────────────────────────┘
```

## 7. Where Auth Happens in System Design

```
                    ┌────────────────────────────┐
Internet ──HTTPS──► │       API Gateway           │
                    │  1. Validate JWT signature   │
                    │  2. Check token expiry        │
                    │  3. Extract user_id, role     │
                    │  4. Rate limit by user        │
                    └──────────┬─────────────────┘
                               │ X-User-Id: 42
                               │ X-User-Role: admin
                               ▼
                    ┌────────────────────────────┐
                    │     Backend Service          │
                    │  5. Check authorization       │
                    │     "Can admin delete this?"  │
                    │  6. Business logic             │
                    └──────────────────────────────┘

Auth at the gateway: validate ONCE, forward identity headers.
AuthZ at the service: each service owns its permission logic.
```

## Interview Checklist

When discussing auth in a system design interview:

- [ ] **Authentication method**: JWT? Session? OAuth? API key?
- [ ] **Token storage**: Where does the client store tokens?
- [ ] **Token lifetime**: Access token TTL? Refresh flow?
- [ ] **Revocation**: How to invalidate a compromised token?
- [ ] **Authorization model**: RBAC? ABAC? Per-resource?
- [ ] **Service-to-service**: mTLS? Service tokens? Trust boundary?
- [ ] **Where auth happens**: Gateway? Each service? Both?
- [ ] **Password storage**: bcrypt/argon2 (never plaintext!)

## Common Interview Questions

### "How would you design the auth system?"

> "I'd use JWT for authentication at the API gateway — it validates the token signature without a DB call, which scales well. Access tokens expire in 15 minutes, refresh tokens in 7 days stored in HttpOnly cookies. For authorization, I'd use RBAC with roles stored in the JWT claims. The gateway extracts the user_id and role, forwarding them as headers to downstream services. Each service makes its own authorization decisions based on these headers."

### "How do you handle token revocation?"

> "Since JWTs are stateless, revocation is the main tradeoff. I'd keep access token TTL very short (15 min) so a leaked token has limited blast radius. For immediate revocation (e.g., user reports account compromised), I'd maintain a small token blocklist in Redis, checked at the gateway. The blocklist only stores tokens that haven't expired yet, so it stays small."

### "How do microservices authenticate each other?"

> "I'd use mTLS managed by a service mesh like Istio. Each service gets a certificate automatically, and the mesh handles mutual authentication and encryption. For authorization between services, I'd use a policy engine like OPA that checks if Service A is allowed to call Service B's endpoint."

## Security Basics to Mention

| Topic | What to Say |
|-------|-------------|
| **Password hashing** | bcrypt or argon2, NEVER MD5/SHA256 |
| **HTTPS** | Always, everywhere, no exceptions |
| **CORS** | Restrict origins, don't use `*` in production |
| **CSRF** | SameSite cookies + CSRF token for form submissions |
| **Rate limiting** | Per-user and per-IP at the gateway |
| **Least privilege** | Services only get permissions they need |

---

## 8. CORS (Cross-Origin Resource Sharing)

**Problem**: Browser blocks `frontend.com` from calling `api.backend.com` by default
(same-origin policy). Your API needs to explicitly allow trusted origins.

```
Browser at https://myapp.com makes fetch() to https://api.myapp.com:

1. Browser sends preflight: OPTIONS /api/data
2. Server responds with:
     Access-Control-Allow-Origin: https://myapp.com    ← "you're allowed"
     Access-Control-Allow-Methods: GET, POST, PUT
     Access-Control-Allow-Headers: Authorization, Content-Type
3. Browser proceeds with the actual request ✓
```

### Why `Access-Control-Allow-Origin: *` is dangerous

```
Your API at api.mybank.com with: Access-Control-Allow-Origin: *

ANY site can now call your API:
  1. User visits evil.com
  2. evil.com's JavaScript calls api.mybank.com/transfer
  3. Browser sends the request WITH the user's bank cookies
  4. API processes it — money gone

With specific origin:
  1. evil.com tries to call api.mybank.com/transfer
  2. Browser checks: is evil.com in Access-Control-Allow-Origin?
  3. No → browser BLOCKS the response ✓
```

### Proper CORS setup

```
# Good — whitelist specific origins
Access-Control-Allow-Origin: https://myapp.com
Access-Control-Allow-Methods: GET, POST, PUT, DELETE
Access-Control-Allow-Headers: Authorization, Content-Type
Access-Control-Allow-Credentials: true     # needed for cookies

# Bad — allows everyone
Access-Control-Allow-Origin: *
# (also: can't use * with Allow-Credentials: true — browser rejects it)
```

**When `*` is OK**: Public read-only APIs with no auth (weather API, CDN fonts, open data).

---

## 9. CSRF (Cross-Site Request Forgery)

**Problem**: User is logged into `bank.com`. They visit `evil.com`, which silently submits
a form to `bank.com`. Browser attaches bank's session cookie automatically → bank thinks
it's the real user.

```
On evil.com:
  <form action="https://bank.com/transfer" method="POST">
    <input type="hidden" name="to" value="attacker" />
    <input type="hidden" name="amount" value="10000" />
  </form>
  <script>document.forms[0].submit();</script>

User visits evil.com → browser auto-submits to bank.com
→ bank sees valid session cookie → transfers $10,000 to attacker
```

### Defense 1: SameSite Cookies

```
Set-Cookie: session=abc123; SameSite=Lax; Secure; HttpOnly
```

| SameSite value | Cookie sent from evil.com → bank.com? |
|---|---|
| `None` | Yes — vulnerable! |
| `Lax` | Only for top-level GET navigation, blocks POST forms ✓ |
| `Strict` | Never sent cross-site (safest, but breaks OAuth redirects) |

`Lax` is the modern browser default and blocks the POST attack above.

### Defense 2: CSRF Token

Server embeds a random token in every form. Attacker can't read it (same-origin policy).

```
bank.com's real form:
  <form action="/transfer" method="POST">
    <input type="hidden" name="csrf_token" value="a8f3b2c9d1e4..." />
    <input name="to" />
    <input name="amount" />
  </form>

Server checks: POST csrf_token matches what I generated?
  bank.com form  → has the real token → ✓
  evil.com form  → can't read bank.com's page → doesn't know the token → ✗
```

### Best practice: use both

```
Set-Cookie: session=abc123; SameSite=Lax; Secure; HttpOnly
+ CSRF token in every state-changing form/request
```

| Attack vector | SameSite=Lax | CSRF token | Both |
|---|---|---|---|
| POST form from evil.com | Blocked ✓ | Blocked ✓ | Blocked ✓ |
| GET link from evil.com | Cookie sent | No token → blocked | Blocked ✓ |
| Subdomain attack | May bypass | Blocked ✓ | Blocked ✓ |
| Old browser (no SameSite) | Vulnerable | Blocked ✓ | Blocked ✓ |

### CORS vs CSRF — they're different things

```
CORS  = server tells browser which OTHER SITES can call its API
CSRF  = attacker tricks YOUR BROWSER into making requests you didn't intend

CORS protects API endpoints (fetch/XHR calls)
CSRF protects form submissions and cookie-authenticated actions
Both are needed — they're complementary, not alternatives
```

---

## 10. Password Storage

**Never** store passwords in plaintext or with fast hashes (MD5, SHA-256).

```
Plaintext:  password123          ← database leak = game over
MD5:        482c811da5d5b4bc...  ← cracked in seconds (GPU does 10B/sec)
SHA-256:    ef92b778bafe77...    ← cracked in seconds (still too fast)
bcrypt:     $2b$12$LJ3m4ys...   ← takes ~250ms per attempt (100B years to brute-force)
argon2:     $argon2id$v=19$...   ← ~500ms + 64MB RAM per attempt (GPU can't parallelize)
```

**Why bcrypt/argon2?** They're **intentionally slow**:

| | SHA-256 | bcrypt | argon2 |
|---|---|---|---|
| Speed (GPU) | 10 billion/sec | ~30,000/sec | ~1,000/sec |
| 8-char passwords | Cracked in ~20 sec | ~200 years | ~6,000 years |
| Memory per hash | ~0 bytes | ~4 KB | 64 MB+ (configurable) |
| GPU resistant | No | Somewhat | Very (needs lots of RAM) |

```
# Good
password_hash = argon2.hash(password, memory=65536, iterations=3)

# Bad
password_hash = sha256(password + salt)   # still 10 billion guesses/sec on GPU
```

**argon2** is the current best practice (won the 2015 Password Hashing Competition).
**bcrypt** is still fine and more widely deployed.
