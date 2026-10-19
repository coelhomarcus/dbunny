-- A small shop: the fixture of the catalog, data, structure and console server tests.
-- Runs in an empty database on MySQL 8.0 and 8.4.

CREATE TABLE customers (
    id INT NOT NULL AUTO_INCREMENT,
    email VARCHAR(120) NOT NULL,
    name VARCHAR(80) NOT NULL DEFAULT 'anonymous',
    age INT NULL,
    bio TEXT,
    PRIMARY KEY (id),
    UNIQUE KEY customers_email_uq (email),
    CONSTRAINT customers_age_ck CHECK (age IS NULL OR age >= 0),
    FULLTEXT KEY customers_bio_ft (bio)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COMMENT = 'People who buy things';

CREATE TABLE orders (
    id INT NOT NULL AUTO_INCREMENT,
    customer_id INT NOT NULL,
    status ENUM('new', 'paid', 'shipped') NOT NULL DEFAULT 'new',
    total DECIMAL(10, 2) NOT NULL DEFAULT 0.00,
    tax_rate DECIMAL(4, 2) NOT NULL DEFAULT 0.20,
    total_with_tax DECIMAL(12, 2) GENERATED ALWAYS AS (total * (1 + tax_rate)) STORED,
    placed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    KEY orders_customer_status_idx (customer_id, status),
    CONSTRAINT orders_customer_fk FOREIGN KEY (customer_id) REFERENCES customers (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE VIEW big_orders AS
SELECT id, customer_id, total FROM orders WHERE total >= 100;

CREATE PROCEDURE touch_customer(IN id INT)
BEGIN
    UPDATE customers SET bio = bio WHERE customers.id = id;
END;

CREATE FUNCTION order_total(o INT) RETURNS DECIMAL(10, 2)
READS SQL DATA
BEGIN
    DECLARE t DECIMAL(10, 2);
    SELECT total INTO t FROM orders WHERE orders.id = o;
    RETURN IFNULL(t, 0);
END;

CREATE TRIGGER orders_bi BEFORE INSERT ON orders FOR EACH ROW
BEGIN
    IF NEW.total < 0 THEN
        SET NEW.total = 0;
    END IF;
END;
