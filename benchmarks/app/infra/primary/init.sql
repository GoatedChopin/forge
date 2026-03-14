-- Replication user for replicas
CREATE ROLE repl_user WITH REPLICATION LOGIN PASSWORD 'repl_password';

-- Allow replication connections
SELECT pg_reload_conf();
