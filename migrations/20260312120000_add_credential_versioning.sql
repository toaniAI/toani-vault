-- ============================================================================
-- 凭证版本控制迁移脚本
-- CredBridge MVP 1.0 - EP2 Story 2.4
--
-- 迁移内容:
-- 1. 添加 version 字段到 credentials 表
-- 2. 创建 credential_versions 历史表
-- 3. 创建相关索引
-- 4. 添加审计日志扩展字段
--
-- 创建日期：2026-03-12
-- 迁移类型：Schema 变更
-- 回滚：可能（需要删除表和字段）
-- ============================================================================

BEGIN;

-- ============================================================================
-- 1. 添加 version 字段到 credentials 表
-- ============================================================================

-- 如果 version 字段不存在，则添加
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'credentials' AND column_name = 'version'
    ) THEN
        ALTER TABLE credentials
        ADD COLUMN version INTEGER NOT NULL DEFAULT 1;

        RAISE NOTICE '已添加 version 字段到 credentials 表';
    ELSE
        RAISE NOTICE 'version 字段已存在，跳过';
    END IF;
END $$;

-- 添加检查约束，确保 version >= 1
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.constraint_table_usage
        WHERE table_name = 'credentials' AND constraint_name = 'credentials_version_check'
    ) THEN
        ALTER TABLE credentials
        ADD CONSTRAINT credentials_version_check CHECK (version >= 1);

        RAISE NOTICE '已添加 version 检查约束';
    ELSE
        RAISE NOTICE 'version 检查约束已存在，跳过';
    END IF;
END $$;

-- ============================================================================
-- 2. 创建 credential_versions 历史表
-- ============================================================================

-- 创建历史表
CREATE TABLE IF NOT EXISTS credential_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    credential_id UUID NOT NULL,
    version INTEGER NOT NULL,
    encrypted_payload JSONB NOT NULL,
    change_reason TEXT,
    changed_by UUID,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- 约束
    CONSTRAINT unique_credential_version UNIQUE (credential_id, version),
    CONSTRAINT credential_versions_version_check CHECK (version >= 1),
    CONSTRAINT fk_credential_versions_credential
        FOREIGN KEY (credential_id) REFERENCES credentials(credential_id) ON DELETE CASCADE
);

-- 添加注释
COMMENT ON TABLE credential_versions IS '凭证版本历史表 - 存储所有凭证的历史版本';
COMMENT ON COLUMN credential_versions.credential_id IS '关联的凭证 ID';
COMMENT ON COLUMN credential_versions.version IS '版本号 (从 1 开始递增)';
COMMENT ON COLUMN credential_versions.encrypted_payload IS '加密的凭证载荷 (JSONB 格式)';
COMMENT ON COLUMN credential_versions.change_reason IS '变更原因 (用于审计)';
COMMENT ON COLUMN credential_versions.changed_by IS '变更人用户 ID (用于审计)';
COMMENT ON COLUMN credential_versions.created_at IS '版本创建时间';

-- ============================================================================
-- 3. 创建索引
-- ============================================================================

-- 凭证表索引
CREATE INDEX IF NOT EXISTS idx_credentials_version
    ON credentials(version);

-- 历史表索引
CREATE INDEX IF NOT EXISTS idx_credential_versions_credential_id
    ON credential_versions(credential_id);

CREATE INDEX IF NOT EXISTS idx_credential_versions_created_at
    ON credential_versions(created_at);

CREATE INDEX IF NOT EXISTS idx_credential_versions_changed_by
    ON credential_versions(changed_by);

-- 复合索引（优化常用查询）
CREATE INDEX IF NOT EXISTS idx_credential_versions_lookup
    ON credential_versions(credential_id, version DESC);

-- ============================================================================
-- 4. 审计日志表扩展
-- ============================================================================

-- 添加 event_category 字段
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'audit_logs' AND column_name = 'event_category'
    ) THEN
        ALTER TABLE audit_logs
        ADD COLUMN event_category VARCHAR(32);

        RAISE NOTICE '已添加 event_category 字段到 audit_logs 表';
    ELSE
        RAISE NOTICE 'event_category 字段已存在，跳过';
    END IF;
END $$;

-- 添加 metadata 字段
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'audit_logs' AND column_name = 'metadata'
    ) THEN
        ALTER TABLE audit_logs
        ADD COLUMN metadata JSONB;

        RAISE NOTICE '已添加 metadata 字段到 audit_logs 表';
    ELSE
        RAISE NOTICE 'metadata 字段已存在，跳过';
    END IF;
END $$;

-- 审计日志索引
CREATE INDEX IF NOT EXISTS idx_audit_logs_category
    ON audit_logs(event_category);

CREATE INDEX IF NOT EXISTS idx_audit_logs_metadata
    ON audit_logs USING GIN (metadata);

-- ============================================================================
-- 5. 迁移现有凭证数据
-- ============================================================================

-- 为现有凭证设置初始版本号（如果尚未设置）
UPDATE credentials
SET version = 1
WHERE version IS NULL OR version = 0;

-- ============================================================================
-- 6. 创建版本历史初始记录（可选）
-- ============================================================================

-- 为现有凭证创建初始版本历史记录
-- 注意：这会为所有现有凭证创建 version=1 的历史记录
-- 如果不需要，可以跳过此步骤

INSERT INTO credential_versions (credential_id, version, encrypted_payload, change_reason, created_at)
SELECT
    id AS credential_id,
    1 AS version,
    json_build_object(
        'version', COALESCE(version, 1),
        'algorithm', COALESCE(algorithm, 'AES-256-GCM'),
        'kdf', COALESCE(kdf, 'HKDF-SHA-256'),
        'nonce', nonce,
        'auth_tag', auth_tag,
        'ciphertext', ciphertext
    ) AS encrypted_payload,
    '初始迁移 - 系统自动创建' AS change_reason,
    created_at
FROM credentials
ON CONFLICT (credential_id, version) DO NOTHING;

-- ============================================================================
-- 7. 创建视图（可选 - 用于简化查询）
-- ============================================================================

-- 创建最新版本视图
CREATE OR REPLACE VIEW credential_latest_versions AS
SELECT
    cv.credential_id,
    cv.version,
    cv.encrypted_payload,
    cv.change_reason,
    cv.changed_by,
    cv.created_at
FROM credential_versions cv
INNER JOIN (
    SELECT credential_id, MAX(version) AS max_version
    FROM credential_versions
    GROUP BY credential_id
) latest ON cv.credential_id = latest.credential_id AND cv.version = latest.max_version;

COMMENT ON VIEW credential_latest_versions IS '凭证最新版本视图 - 快速查询每个凭证的最新版本';

-- ============================================================================
-- 8. 创建触发器函数（自动保存版本历史）
-- ============================================================================

-- 创建触发器函数：当凭证更新时自动保存历史版本
CREATE OR REPLACE FUNCTION save_credential_version_history()
RETURNS TRIGGER AS $$
BEGIN
    -- 仅当 version 变更时保存历史
    IF NEW.version > OLD.version THEN
        INSERT INTO credential_versions (
            credential_id,
            version,
            encrypted_payload,
            change_reason,
            changed_by,
            created_at
        ) VALUES (
            OLD.id,
            OLD.version,
            json_build_object(
                'version', OLD.version,
                'algorithm', OLD.algorithm,
                'kdf', OLD.kdf,
                'nonce', OLD.nonce,
                'auth_tag', OLD.auth_tag,
                'ciphertext', OLD.ciphertext
            ),
            OLD.change_reason,
            OLD.changed_by,
            OLD.updated_at
        );
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION save_credential_version_history() IS '凭证版本历史自动保存触发器函数';

-- ============================================================================
-- 迁移完成
-- ============================================================================

COMMIT;

-- ============================================================================
-- 回滚脚本（仅用于参考，不要在生产环境执行）
-- ============================================================================

/*
-- 回滚步骤（仅在需要时执行）:

BEGIN;

-- 1. 删除触发器
DROP TRIGGER IF EXISTS tr_save_credential_version_history ON credentials;

-- 2. 删除触发器函数
DROP FUNCTION IF EXISTS save_credential_version_history();

-- 3. 删除视图
DROP VIEW IF EXISTS credential_latest_versions;

-- 4. 删除历史表
DROP TABLE IF EXISTS credential_versions;

-- 5. 删除字段（谨慎操作，会丢失数据）
ALTER TABLE credentials DROP COLUMN IF EXISTS version;

-- 6. 恢复 audit_logs 表
ALTER TABLE audit_logs DROP COLUMN IF EXISTS event_category;
ALTER TABLE audit_logs DROP COLUMN IF EXISTS metadata;

COMMIT;
*/
