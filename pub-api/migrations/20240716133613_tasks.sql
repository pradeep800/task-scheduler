

CREATE TABLE Tasks (
    id SERIAL PRIMARY KEY, 
    schedule_at_in_second INTEGER NOT NULL,
    status VARCHAR(256) NOT NULL, 
    output TEXT NOT NULL,  
    retry SMALLINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL 
);
