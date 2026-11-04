CREATE TABLE alpha(id INTEGER PRIMARY KEY, "名,称" TEXT, body TEXT);
WITH RECURSIVE records(n) AS (
    SELECT 1 UNION ALL SELECT n + 1 FROM records WHERE n < 520
)
INSERT INTO alpha
SELECT n, '中文 ' || n,
    CASE WHEN n = 1 THEN 'line 1' || char(10) || 'line 2 "yes"'
         ELSE printf('record %03d', n) END
FROM records;
CREATE TABLE beta(value TEXT);
INSERT INTO beta VALUES ('retained beta');
