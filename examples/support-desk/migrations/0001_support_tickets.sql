-- @up
CREATE TYPE ticket_status AS ENUM ('new', 'working', 'resolved');
CREATE TYPE ticket_priority AS ENUM ('low', 'normal', 'high');

CREATE TABLE support_tickets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_name TEXT NOT NULL,
    title TEXT NOT NULL,
    details TEXT NOT NULL,
    status ticket_status NOT NULL DEFAULT 'new',
    priority ticket_priority NOT NULL DEFAULT 'normal',
    last_note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_support_tickets_status_updated_at
    ON support_tickets (status, updated_at DESC);

SELECT forge_enable_reactivity('support_tickets');

-- @down
SELECT forge_disable_reactivity('support_tickets');
DROP TABLE IF EXISTS support_tickets;
DROP TYPE IF EXISTS ticket_priority;
DROP TYPE IF EXISTS ticket_status;
