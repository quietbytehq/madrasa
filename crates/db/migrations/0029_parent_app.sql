-- Parent app, stage 1 (docs/adr/0006-parent-app.md): school notices and messages between a
-- guardian and the class teacher. These are school records: they sync with the hub, where
-- the parent app reads and writes them. Empty and unused in a school that has not joined.

-- A notice for parents: the whole school, or chosen classes.
CREATE TABLE notices (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    title         TEXT NOT NULL CHECK (btrim(title) <> ''),
    body          TEXT NOT NULL DEFAULT '',
    -- Classes the notice is for; empty means the whole school.
    class_ids     UUID[] NOT NULL DEFAULT '{}',
    -- NULL while it is a draft or after it was withdrawn; parents see published notices.
    published_at  TIMESTAMPTZ,
    -- Last day the notice is shown; NULL means until it is withdrawn.
    expires_on    DATE,
    created_by    UUID REFERENCES users (id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL
);
CREATE INDEX notices_school_idx ON notices (school_id, published_at DESC) WHERE deleted_at IS NULL;

-- One message in the conversation about one student between one guardian and the class
-- teacher. There is no thread record: a conversation is the messages of a student and a
-- guardian, so neither side can create it twice. Messages are never changed or removed.
CREATE TABLE messages (
    id              UUID PRIMARY KEY,
    school_id       UUID NOT NULL REFERENCES schools (id),
    student_id      UUID NOT NULL REFERENCES students (id),
    guardian_id     UUID NOT NULL REFERENCES guardians (id),
    -- Who wrote it: the guardian (in the parent app) or a member of staff (in the school).
    sender          TEXT NOT NULL CHECK (sender IN ('guardian', 'staff')),
    sender_user_id  UUID REFERENCES users (id),
    -- The writer's name as it was, for the other side to read.
    sender_name     TEXT NOT NULL,
    body            TEXT NOT NULL CHECK (btrim(body) <> '' AND char_length(body) <= 2000),
    sent_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by      UUID,
    deleted_at      TIMESTAMPTZ,
    hlc             TEXT NOT NULL,
    CONSTRAINT messages_staff_login_check CHECK ((sender = 'staff') = (sender_user_id IS NOT NULL))
);
CREATE INDEX messages_thread_idx ON messages (student_id, guardian_id, sent_at);

CREATE FUNCTION messages_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'messages are never changed or removed';
END;
$$;
CREATE TRIGGER messages_no_update BEFORE UPDATE ON messages
    FOR EACH ROW EXECUTE FUNCTION messages_immutable();
CREATE TRIGGER messages_no_delete BEFORE DELETE ON messages
    FOR EACH ROW EXECUTE FUNCTION messages_immutable();

-- How far each side has read a conversation. The guardian's mark is written at the hub, the
-- staff mark at the school: each side only ever writes its own row.
CREATE TABLE message_reads (
    id           UUID PRIMARY KEY,
    school_id    UUID NOT NULL REFERENCES schools (id),
    student_id   UUID NOT NULL REFERENCES students (id),
    guardian_id  UUID NOT NULL REFERENCES guardians (id),
    reader       TEXT NOT NULL CHECK (reader IN ('guardian', 'staff')),
    read_up_to   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by   UUID,
    deleted_at   TIMESTAMPTZ,
    hlc          TEXT NOT NULL
);
CREATE UNIQUE INDEX message_reads_uq ON message_reads (student_id, guardian_id, reader) WHERE deleted_at IS NULL;

-- Grant the parent app permissions to the built-in roles of existing schools.
--
-- A joined school has a copy at its hub, and this migration runs on both. The grants get
-- an id computed from the role and the permission and a fixed date, so both copies hold the
-- very same records, and change capture is silenced for these inserts so that neither copy
-- tries to send them to the other (two different records for one grant could not both be
-- stored).
SET LOCAL madrasa.sync_apply = 'on';
INSERT INTO permissions (id, school_id, role_id, code, created_at, updated_at, hlc)
SELECT md5(r.id::text || ':' || p.code)::uuid, r.school_id, r.id, p.code,
       TIMESTAMPTZ '2026-11-12 00:00:00+00', TIMESTAMPTZ '2026-11-12 00:00:00+00',
       '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'parents.manage'), ('principal', 'notices.manage'),
             ('principal', 'messages.reply'), ('principal', 'messages.view_all'),
             ('admin', 'parents.manage'), ('admin', 'notices.manage'),
             ('teacher', 'messages.reply')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
ON CONFLICT DO NOTHING;
SET LOCAL madrasa.sync_apply = 'off';
