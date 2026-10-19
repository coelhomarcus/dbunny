-- One column of every type the grid knows (spec 6.2.1) and two rows: the first ordinary,
-- the second the extremes. Runs on MySQL 8.0 and 8.4.
-- Zero dates need a session without NO_ZERO_DATE and the strict modes.
SET SESSION sql_mode = '';

CREATE TABLE all_types (
    id INT NOT NULL PRIMARY KEY,
    c_tinyint TINYINT,
    c_tinyint_u TINYINT UNSIGNED,
    c_smallint SMALLINT,
    c_mediumint MEDIUMINT,
    c_int INT,
    c_int_u INT UNSIGNED,
    c_bigint BIGINT,
    c_bigint_u BIGINT UNSIGNED,
    c_decimal DECIMAL(20, 4),
    c_float FLOAT,
    c_double DOUBLE,
    c_date DATE,
    c_datetime DATETIME(6),
    c_timestamp TIMESTAMP NULL,
    c_time TIME(3),
    c_year YEAR,
    c_bit8 BIT(8),
    c_bit64 BIT(64),
    c_json JSON,
    c_binary BINARY(4),
    c_varbinary VARBINARY(16),
    c_blob BLOB,
    c_char CHAR(8),
    c_varchar VARCHAR(40),
    c_text TEXT,
    c_enum ENUM('small', 'medium', 'large'),
    c_set SET('a', 'b', 'c'),
    c_geometry POINT,
    c_null INT
) DEFAULT CHARSET = utf8mb4;

INSERT INTO all_types VALUES
(1, -5, 200, 300, -70000, 123456, 4000000000, 42, 7, 12.5, 1.5, -2.25,
 '2024-02-29', '2024-02-29 13:45:10.123456', '2024-02-29 13:45:10', '12:34:56.789', 2024,
 b'00000101', b'101', '{"a": [1, 2]}', 0x010203, 0x61, 0x00ff, 'abc', 'hello', 'some text',
 'small', 'a,c', ST_GeomFromText('POINT(1 2)'), NULL),
(2, -128, 255, -32768, 8388607, -2147483648, 4294967295, -9223372036854775808, 18446744073709551615,
 -9999999999999999.9999, -1e10, 1.7976931348623157e308,
 '0000-00-00', '0000-00-00 00:00:00.000000', '2038-01-19 03:14:07', '-838:59:59.000', 2155,
 b'11111111', b'1111111111111111111111111111111111111111111111111111111111111111',
 '{"a": {"b": [1, {"c": null}], "d": "x"}, "e": true, "f": "9007199254740993"}',
 0x00ff00ff, '', 0x00ff, 'olá', 'it''s "q" \\ ç 日本', '😀 text',
 'large', '', ST_GeomFromText('POINT(-3.5 0)'), NULL);
