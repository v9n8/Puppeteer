-- Lookups the routing code runs often (QO-06): every finished run looks for
-- its open worker row by worker_run_id (`escalate::close_and_read`), and the
-- policy update reads rows by finished_at (`update::finished_since`). Without
-- these both scan the whole table, which only grows.
CREATE INDEX idx_routing_decisions_worker ON routing_decisions(worker_run_id);
CREATE INDEX idx_routing_decisions_finished ON routing_decisions(finished_at);
