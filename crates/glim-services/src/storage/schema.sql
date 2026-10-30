CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE paths(scope TEXT NOT NULL, owner TEXT NOT NULL, position INTEGER NOT NULL, path TEXT NOT NULL, PRIMARY KEY(scope,owner,position));
CREATE TABLE recent_workspaces(id TEXT PRIMARY KEY, name TEXT, position INTEGER NOT NULL UNIQUE);
CREATE TABLE session_groups(scope TEXT NOT NULL, id TEXT NOT NULL, PRIMARY KEY(scope,id));
CREATE TABLE sessions(id INTEGER PRIMARY KEY, scope TEXT NOT NULL, owner TEXT NOT NULL, position INTEGER NOT NULL, workspace_id TEXT, name TEXT, window_group TEXT, active INTEGER, UNIQUE(scope,owner,position), FOREIGN KEY(scope,owner) REFERENCES session_groups(scope,id) ON DELETE CASCADE);
CREATE TABLE session_roots(session INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE, position INTEGER NOT NULL, path TEXT NOT NULL, PRIMARY KEY(session,position));
CREATE TABLE session_tabs(session INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE, position INTEGER NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('file','playlist')), path TEXT NOT NULL, flag INTEGER NOT NULL CHECK(flag IN (0,1)), PRIMARY KEY(session,position));
-- Versioned reader state and update checkpoints keep their format-specific payloads.
CREATE TABLE records(kind TEXT NOT NULL, id TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(kind,id));
CREATE TABLE drafts(session TEXT NOT NULL, id TEXT NOT NULL, title TEXT NOT NULL, directory TEXT NOT NULL, text TEXT NOT NULL, PRIMARY KEY(session,id));
