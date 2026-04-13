-- ============================================================================
-- Migration: add credential whitelist to api_tokens
-- Created: 2026-04-13
-- Description: Persists resource-level credential restrictions for dashboard-issued sandbox tokens
-- ============================================================================

ALTER TABLE api_tokens
    ADD COLUMN IF NOT EXISTS credential_ids JSONB NOT NULL DEFAULT '[]'::jsonb;
