-- Standard stage 8: directory (Active Directory / LDAP) login. Accounts created from the
-- directory carry their DN and are authenticated by an LDAP bind instead of a local
-- password. Settings hold the directory configuration; the bind password is sealed with
-- the instance key (settings `instance.key`). Nothing changes for Lite installs.

ALTER TABLE users
    ADD COLUMN auth_source TEXT NOT NULL DEFAULT 'local' CHECK (auth_source IN ('local', 'directory')),
    ADD COLUMN directory_dn TEXT,
    ADD COLUMN directory_synced_at TIMESTAMPTZ;
