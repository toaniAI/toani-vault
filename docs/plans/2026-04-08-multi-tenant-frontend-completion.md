# Multi-Tenant Frontend Completion Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the provided multi-tenant session and frontend model so users can browse memberships, switch current tenant, manage tenant-scoped pages, and create a tenant from the UI.

**Architecture:** Keep `memberships[] + currentTenantId` as the only authoritative tenant context. The backend auth API returns full membership context and selection fallbacks; the frontend normalizes that shape once, persists only safe local context, and drives tenant-aware pages, guards, and query invalidation from the store.

**Tech Stack:** Rust + Axum backend, React 19 + TypeScript + Zustand + TanStack Query frontend.

---

### Task 1: Backend auth session shape

**Files:**

- Modify: `src/api/auth.rs`
- Modify: `src/api/middleware.rs`
- Test: `src/api/auth.rs`

**Steps:**

1. Add backend response structs for `memberships`, `current_tenant`, and `current_membership`, and add `default_tenant_id` to `UserProfile`.
2. Extract shared mapping helpers for user profile, membership info, and tenant selection fallback.
3. Update `POST /auth/session` to fetch all active memberships, prioritize invitation/default/first fallback, and return the new shape.
4. Update `GET /auth/me` to select current membership from token tenant, then default tenant, then first active membership.
5. Add `GET /auth/memberships` and return active memberships in the same element shape used elsewhere.
6. Add focused tests for serialization and tenant selection helpers.

### Task 2: Frontend auth normalization and store

**Files:**

- Modify: `frontend/src/shared/api/authSession.ts`
- Modify: `frontend/src/shared/stores/authStore.ts`
- Modify: `frontend/src/shared/api/types.ts`
- Modify: `frontend/src/shared/api/auth-types.ts`
- Test: `frontend/src/shared/api/auth.test.ts`
- Test: `frontend/src/shared/api/auth.test.mjs`

**Steps:**

1. Expand raw auth response types to accept `memberships[]`, `current_tenant`, and `current_membership`.
2. Normalize full backend membership arrays and compute `activeMembership` as a derived value only.
3. Update the auth store so `setSession` validates or derives `currentTenantId` from explicit input, `defaultTenantId`, or first membership.
4. Add store actions for `switchCurrentTenant`, `setMemberships`, and a shared active-membership sync helper.
5. Add tests covering session normalization, tenant switching, and membership refresh behavior.

### Task 3: Frontend tenant API and query invalidation

**Files:**

- Modify: `frontend/src/shared/api/auth-hooks.ts`
- Modify: `frontend/src/shared/api/auth-services.ts`
- Modify: `frontend/src/shared/api/hooks.ts`
- Modify: `frontend/src/shared/api/services.ts`
- Add: `frontend/src/shared/api/tenant.ts`

**Steps:**

1. Add a dedicated tenant API client for create/get/config/update operations.
2. Standardize tenant-aware query keys for memberships, tenant, tenant-config, members, and invitations.
3. Add hooks for `useMemberships`, `useTenant`, `useTenantConfig`, `useCreateTenant`, and `useSwitchTenant`.
4. Update invitation acceptance and profile update flows to refresh memberships and synchronize `currentTenantId`.
5. Ensure tenant switch invalidates members, invitations, settings, and other tenant-scoped queries.

### Task 4: Routes, guards, and layout

**Files:**

- Modify: `frontend/src/app/router-components.tsx`
- Modify: `frontend/src/app/Layout.tsx`
- Modify: `frontend/src/app/router.tsx`
- Add: `frontend/src/features/tenants/pages/TenantsPage.tsx`

**Steps:**

1. Add a tenant-aware route guard for pages that require a valid current tenant and tenant-level permissions.
2. Update public-route redirects to send onboarding-complete users with zero memberships to `/tenants`.
3. Add the `/tenants` route and sidebar navigation entry.
4. Add a persistent tenant switcher in layout/header when the user belongs to multiple tenants.
5. Redirect invalid admin access on `/users` and `/settings` back to `/tenants`.

### Task 5: Tenant-scoped pages

**Files:**

- Modify: `frontend/src/features/tenants/pages/UsersPage.tsx`
- Modify: `frontend/src/features/tenants/pages/SettingsPage.tsx`
- Modify: `frontend/src/features/auth/pages/ProfilePage.tsx`
- Modify: `frontend/src/features/auth/pages/OnboardingPage.tsx`
- Modify: `frontend/src/features/auth/pages/LoginPage.tsx`
- Modify: `frontend/src/features/auth/pages/InvitationAcceptPage.tsx`

**Steps:**

1. Refactor `UsersPage` to require `currentTenantId`, show tenant context in the header, and rely on current membership role/scopes instead of member-list inference for page access.
2. Refactor `SettingsPage` into a current-tenant page and load tenant config.
3. Expand `ProfilePage` to show all memberships and support setting the default tenant through profile update.
4. Update `OnboardingPage` to use current tenant summary, membership count, invitation guidance, and `/tenants` fallback.
5. Update login/invitation flows so new memberships refresh and switch into the newly joined tenant.

### Task 6: Verification

**Files:**

- Test: `tests/` auth-related coverage if needed
- Test: `frontend/src/**/*.test.ts`

**Steps:**

1. Run targeted frontend tests for auth normalization and tenant-aware behavior.
2. Run targeted Rust tests for auth response selection logic.
3. Run `cargo fmt`.
4. Run `cargo clippy --tests -- -D warnings`.
5. Run `cargo test`.
6. Run frontend lint/tests that are relevant to touched files.
