CREATE TABLE IF NOT EXISTS peers (
  id TEXT PRIMARY KEY UNIQUE NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW'))
);

--- outgoing transfers
CREATE TABLE IF NOT EXISTS transfers (
  id TEXT PRIMARY KEY UNIQUE NOT NULL,
  peer_id TEXT NOT NULL, 
  is_outgoing INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(peer_id) REFERENCES peers(id) ON DELETE CASCADE ON UPDATE CASCADE,
  CHECK(is_outgoing = 0 OR is_outgoing = 1)
);

-- transfer states
CREATE TABLE IF NOT EXISTS transfer_active_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  transfer_id TEXT NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(transfer_id) REFERENCES transfers(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS transfer_cancel_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  transfer_id TEXT NOT NULL,
  by_peer INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(transfer_id) REFERENCES transfers(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS transfer_failed_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  transfer_id TEXT NOT NULL,
  status_code INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(transfer_id) REFERENCES transfers(id) ON DELETE CASCADE ON UPDATE CASCADE
);

-- all the paths inside the outgoing transfer
CREATE TABLE IF NOT EXISTS outgoing_paths (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  transfer_id TEXT NOT NULL,
  path TEXT NOT NULL,
  path_id TEXT NOT NULL,
  bytes INT NOT NULL, 
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(transfer_id) REFERENCES transfers(id) ON DELETE CASCADE ON UPDATE CASCADE
);

-- all the paths inside the incoming transfer
CREATE TABLE IF NOT EXISTS incoming_paths (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  transfer_id TEXT NOT NULL,   
  path TEXT NOT NULL, 
  path_id TEXT NOT NULL,
  bytes INT NOT NULL, 
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(transfer_id) REFERENCES transfers(id) ON DELETE CASCADE ON UPDATE CASCADE
);

-- states for outgoing paths(uploads)
CREATE TABLE IF NOT EXISTS outgoing_path_pending_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE CASCADE ON UPDATE CASCADE
);
CREATE TABLE IF NOT EXISTS outgoing_path_started_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  bytes_sent INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE CASCADE ON UPDATE CASCADE,
  CHECK(bytes_sent >= 0)
);
CREATE TABLE IF NOT EXISTS outgoing_path_cancel_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  by_peer INTEGER NOT NULL,
  bytes_sent INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE CASCADE ON UPDATE CASCADE,
  CHECK(bytes_sent >= 0)
);
CREATE TABLE IF NOT EXISTS outgoing_path_failed_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  status_code INTEGER NOT NULL,
  bytes_sent INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE CASCADE ON UPDATE CASCADE,
  CHECK(bytes_sent >= 0)
);
CREATE TABLE IF NOT EXISTS outgoing_path_completed_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE CASCADE ON UPDATE CASCADE
);

-- states for incoming paths(downloads)
CREATE TABLE IF NOT EXISTS incoming_path_pending_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE CASCADE ON UPDATE CASCADE
);
CREATE TABLE IF NOT EXISTS incoming_path_started_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  bytes_received INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE CASCADE ON UPDATE CASCADE,
  CHECK(bytes_received >= 0)
);
CREATE TABLE IF NOT EXISTS incoming_path_cancel_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  by_peer INTEGER NOT NULL,
  bytes_received INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE CASCADE ON UPDATE CASCADE,
  CHECK(bytes_received >= 0)
);
CREATE TABLE IF NOT EXISTS incoming_path_failed_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  status_code INTEGER NOT NULL,
  bytes_received INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE CASCADE ON UPDATE CASCADE,
  CHECK(bytes_received >= 0)
);
CREATE TABLE IF NOT EXISTS incoming_path_completed_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  final_path TEXT NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE CASCADE ON UPDATE CASCADE
);

-- TODO: indexes
