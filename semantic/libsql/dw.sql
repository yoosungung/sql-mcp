PRAGMA foreign_keys = ON;

CREATE TABLE customer (
  customer_id INTEGER NOT NULL PRIMARY KEY,
  region_code TEXT NOT NULL,
  status_code TEXT NOT NULL
) STRICT;

CREATE TABLE orders (
  order_id INTEGER NOT NULL PRIMARY KEY,
  customer_id INTEGER NOT NULL REFERENCES customer (customer_id),
  region_code TEXT NOT NULL,
  status_code TEXT NOT NULL,
  order_date TEXT NOT NULL,
  sale_amount REAL NOT NULL,
  fx_rate REAL NOT NULL
) STRICT;

CREATE TABLE orders_month (
  order_month TEXT NOT NULL,
  region_code TEXT NOT NULL,
  status_code TEXT NOT NULL,
  sale_amount REAL NOT NULL,
  fx_rate REAL NOT NULL,
  PRIMARY KEY (order_month, region_code, status_code)
) STRICT;
