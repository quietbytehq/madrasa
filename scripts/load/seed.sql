-- A large made-up school for the load test (scripts/load-test.sh): about 5,000 students in
-- 144 sections, 250 staff, and three academic years of fee vouchers, receipts, attendance
-- and marks. Run with psql on a database where setup has been done and the academic year
-- 2026-27 is active and empty. Everything is generated; no name here is a real person.
--
--   psql "$URL" -v ON_ERROR_STOP=1 -v per_section=35 -f scripts/load/seed.sql
\set ON_ERROR_STOP on
\if :{?per_section} \else \set per_section 35 \endif
\timing off
BEGIN;

-- A time-ordered id shaped like UUID v7 (the app's own ids are real v7).
CREATE FUNCTION pg_temp.uid() RETURNS uuid LANGUAGE sql VOLATILE AS $$
  SELECT (lpad(to_hex((extract(epoch FROM clock_timestamp()) * 1000)::bigint), 12, '0')
          || '7' || substr(md5(random()::text), 1, 3)
          || to_hex(8 + floor(random() * 4)::int) || substr(md5(random()::text), 1, 15))::uuid
$$;

CREATE TEMP TABLE ctx AS
SELECT s.id AS school_id,
       (SELECT id FROM users WHERE school_id = s.id ORDER BY created_at LIMIT 1) AS admin_id,
       (SELECT id FROM academic_years WHERE school_id = s.id AND is_active AND deleted_at IS NULL) AS y3
FROM schools s LIMIT 1;
DO $$ BEGIN
  IF (SELECT y3 FROM ctx) IS NULL THEN
    RAISE EXCEPTION 'Set the school up and make an academic year active first.';
  END IF;
  IF EXISTS (SELECT 1 FROM students) THEN
    RAISE EXCEPTION 'This database already has students: the load data is for an empty school.';
  END IF;
END $$;

-- ---- years, classes, sections, subjects -------------------------------------------------------
INSERT INTO academic_years (id, school_id, name, start_date, end_date, start_month, is_active, hlc)
SELECT pg_temp.uid(), school_id, n, s, e, 4, FALSE, 'load'
FROM ctx, (VALUES ('2024-25', DATE '2024-04-01', DATE '2025-03-31'),
                  ('2025-26', DATE '2025-04-01', DATE '2026-03-31')) v(n, s, e);

CREATE TEMP TABLE yrs AS
SELECT id, name, start_date, end_date, row_number() OVER (ORDER BY start_date) AS no
FROM academic_years WHERE school_id = (SELECT school_id FROM ctx) AND deleted_at IS NULL;

INSERT INTO classes (id, school_id, name, level, hlc)
SELECT pg_temp.uid(), school_id, n, l, 'load'
FROM ctx, (VALUES ('Nursery', -1), ('KG', 0), ('1', 1), ('2', 2), ('3', 3), ('4', 4), ('5', 5),
                  ('6', 6), ('7', 7), ('8', 8), ('9', 9), ('10', 10)) v(n, l);

INSERT INTO subjects (id, school_id, name, code, hlc)
SELECT pg_temp.uid(), school_id, n, c, 'load'
FROM ctx, (VALUES ('English', 'ENG'), ('Urdu', 'URD'), ('Mathematics', 'MAT'), ('Science', 'SCI'),
                  ('Islamiat', 'ISL'), ('Social Studies', 'SST'), ('Computer', 'COM'), ('Art', 'ART')) v(n, c);

INSERT INTO class_subjects (id, school_id, class_id, subject_id, hlc)
SELECT pg_temp.uid(), c.school_id, c.id, s.id, 'load' FROM classes c JOIN subjects s ON s.school_id = c.school_id;

INSERT INTO staff (id, school_id, staff_number, full_name, father_name, gender, phone, designation, joined_on, status, hlc)
SELECT pg_temp.uid(), school_id, 'LOAD-T-' || lpad(g::text, 4, '0'), 'Teacher ' || g, 'Father ' || g,
       CASE WHEN g % 2 = 0 THEN 'female' ELSE 'male' END, '0300' || lpad((2000000 + g)::text, 7, '0'),
       'Teacher', DATE '2020-08-01', 'active', 'load'
FROM ctx, generate_series(1, 250) g;

CREATE TEMP TABLE staff_no AS
SELECT id, row_number() OVER (ORDER BY staff_number) AS no FROM staff WHERE staff_number LIKE 'LOAD-T-%';

INSERT INTO sections (id, school_id, class_id, academic_year_id, name, class_teacher_id, capacity, hlc)
SELECT pg_temp.uid(), c.school_id, c.id, y.id, chr(64 + g),
       (SELECT id FROM staff_no WHERE no = ((c.level + 1) * 12 + g) % 250 + 1), 40, 'load'
FROM classes c, yrs y, generate_series(1, 12) g;

-- ---- students, guardians, enrolment ------------------------------------------------------------
CREATE TEMP TABLE sec3 AS
SELECT s.id, s.class_id, c.level, c.name AS class_name, s.name,
       row_number() OVER (ORDER BY c.level, s.name) AS no
FROM sections s JOIN classes c ON c.id = s.class_id WHERE s.academic_year_id = (SELECT y3 FROM ctx);

CREATE TEMP TABLE stu AS
SELECT pg_temp.uid() AS id, sec.no AS sec_no, sec.class_id, sec.name AS sec_name, g AS roll,
       ((sec.no - 1) * :per_section + g)::int AS n
FROM sec3 sec, generate_series(1, :per_section) g;

INSERT INTO students (id, school_id, student_number, legacy_id, full_name, father_name, date_of_birth, gender,
                      address, admission_date, status, hlc)
SELECT s.id, c.school_id, 'LOAD-' || lpad(s.n::text, 5, '0'), 'OLD-' || s.n,
       (ARRAY['Ali','Sara','Omar','Hina','Bilal','Zara','Usman','Noor','Hamza','Ayesha','Saad','Maryam'])[1 + s.n % 12]
         || ' ' || (ARRAY['Khan','Raza','Shah','Malik','Qureshi','Sheikh','Javed','Iqbal','Baig','Tariq'])[1 + (s.n / 12) % 10]
         || ' ' || s.n,
       'Father ' || ((s.n + 2) / 3), DATE '2010-01-01' + (s.n % 3000), CASE WHEN s.n % 2 = 0 THEN 'female' ELSE 'male' END,
       'House ' || s.n || ', Street ' || (s.n % 40), DATE '2024-04-01', 'active', 'load'
FROM stu s, ctx c;

-- One guardian for every three children (brothers and sisters share a parent).
CREATE TEMP TABLE gua AS SELECT pg_temp.uid() AS id, g AS no FROM generate_series(1, (SELECT (max(n) + 2) / 3 FROM stu)) g;
INSERT INTO guardians (id, school_id, full_name, phone, occupation, address, hlc)
SELECT g.id, c.school_id, 'Father ' || g.no, '0301' || lpad((1000000 + g.no)::text, 7, '0'), 'Trader', 'House ' || g.no, 'load'
FROM gua g, ctx c;
INSERT INTO student_guardians (id, school_id, student_id, guardian_id, relationship, is_primary, hlc)
SELECT pg_temp.uid(), c.school_id, s.id, g.id, 'father', TRUE, 'load'
FROM stu s JOIN gua g ON g.no = (s.n + 2) / 3, ctx c;

-- The same section letter of the same class in each year (a school that does not promote is
-- enough for a load test: the row counts are what matter).
INSERT INTO enrollments (id, school_id, student_id, academic_year_id, section_id, roll_no, hlc)
SELECT pg_temp.uid(), c.school_id, s.id, sec.academic_year_id, sec.id, s.roll, 'load'
FROM stu s JOIN sections sec ON sec.class_id = s.class_id AND sec.name = s.sec_name, ctx c;

-- ---- fees: structure, 32 months of vouchers, receipts for nine in ten ------------------------
INSERT INTO fee_heads (id, school_id, name, code, is_recurring, position, hlc)
SELECT pg_temp.uid(), school_id, n, c, TRUE, p, 'load'
FROM ctx, (VALUES ('Tuition fee', 'TUITION', 1), ('Computer fee', 'COMPUTER', 2)) v(n, c, p);

INSERT INTO fee_structures (id, school_id, academic_year_id, class_id, fee_head_id, amount, hlc)
SELECT pg_temp.uid(), c.school_id, y.id, c.id, h.id,
       CASE h.code WHEN 'TUITION' THEN 2500 + 150 * (c.level + 1) ELSE 300 END, 'load'
FROM classes c, yrs y, fee_heads h WHERE h.code IN ('TUITION', 'COMPUTER');

CREATE TEMP TABLE months AS
SELECT m::date AS month, row_number() OVER (ORDER BY m) AS no,
       (SELECT id FROM yrs y WHERE m::date BETWEEN y.start_date AND y.end_date) AS year_id
FROM generate_series(DATE '2024-04-01', date_trunc('month', current_date) - INTERVAL '1 month', INTERVAL '1 month') m;

CREATE TEMP TABLE vch AS
SELECT pg_temp.uid() AS id, s.id AS student_id, s.n, s.class_id, m.month, m.no AS month_no, m.year_id,
       (s.n + m.no) % 10 <> 0 OR m.no < (SELECT max(no) FROM months) - 2 AS paid
FROM stu s, months m;

INSERT INTO fee_vouchers (id, school_id, student_id, academic_year_id, section_id, voucher_no, billing_month, issue_date,
                          due_date, gross, sibling_discount, concession_discount, net_monthly, arrears, total_due,
                          late_fee, net_paid, status, updated_by, hlc)
SELECT v.id, c.school_id, v.student_id, v.year_id,
       (SELECT e.section_id FROM enrollments e WHERE e.student_id = v.student_id AND e.academic_year_id = v.year_id),
       'L' || to_char(v.month, 'YYYYMM') || '-' || lpad(v.n::text, 5, '0'), v.month, v.month, v.month + 14,
       f.total, 0, 0, f.total, 0, f.total, 0, CASE WHEN v.paid THEN f.total ELSE 0 END,
       CASE WHEN v.paid THEN 'paid' ELSE 'unpaid' END, c.admin_id, 'load'
FROM vch v
JOIN (SELECT class_id, academic_year_id, sum(amount) AS total FROM fee_structures GROUP BY 1, 2) f
  ON f.class_id = v.class_id AND f.academic_year_id = v.year_id, ctx c;

INSERT INTO fee_voucher_lines (id, school_id, voucher_id, fee_head_id, description, amount, position, hlc)
SELECT pg_temp.uid(), c.school_id, v.id, fs.fee_head_id, h.name, fs.amount, h.position, 'load'
FROM vch v JOIN fee_structures fs ON fs.class_id = v.class_id AND fs.academic_year_id = v.year_id
JOIN fee_heads h ON h.id = fs.fee_head_id, ctx c;

INSERT INTO fee_receipts (id, school_id, voucher_id, student_id, receipt_no, paid_on, amount, method, received_by, hlc)
SELECT pg_temp.uid(), c.school_id, v.id, v.student_id, 'LR' || to_char(v.month, 'YYYYMM') || '-' || lpad(v.n::text, 5, '0'),
       v.month + 5 + (v.n % 9), fv.total_due, CASE WHEN v.n % 4 = 0 THEN 'bank' ELSE 'cash' END, c.admin_id, 'load'
FROM vch v JOIN fee_vouchers fv ON fv.id = v.id, ctx c WHERE v.paid;

-- ---- attendance: every school day (Monday to Friday) of the three years -----------------------
CREATE TEMP TABLE days AS
SELECT d::date AS day, (SELECT id FROM yrs y WHERE d::date BETWEEN y.start_date AND y.end_date) AS year_id
FROM generate_series(DATE '2024-04-01', current_date - 1, INTERVAL '1 day') d
WHERE extract(isodow FROM d) < 6 AND extract(month FROM d) NOT IN (6, 7);

CREATE TEMP TABLE sht AS
SELECT pg_temp.uid() AS id, sec.id AS section_id, sec.class_id, sec.name, d.day, d.year_id
FROM sections sec JOIN days d ON d.year_id = sec.academic_year_id;

INSERT INTO attendance_sheets (id, school_id, section_id, academic_year_id, date, marked_by, submitted_at,
                               present, absent, late, leave, hlc)
SELECT s.id, c.school_id, s.section_id, s.year_id, s.day, c.admin_id, s.day + TIME '08:30',
       :per_section - 2, 1, 1, 0, 'load'
FROM sht s, ctx c;

INSERT INTO attendance_entries (id, school_id, sheet_id, student_id, date, status, hlc)
SELECT pg_temp.uid(), c.school_id, s.id, st.id, s.day,
       CASE (st.roll + extract(doy FROM s.day)::int) % :per_section WHEN 0 THEN 'absent' WHEN 1 THEN 'late' ELSE 'present' END,
       'load'
FROM sht s JOIN stu st ON st.class_id = s.class_id AND st.sec_name = s.name, ctx c;

-- ---- exams: four assessments a year, every subject, every student ---------------------------
INSERT INTO assessment_types (id, school_id, academic_year_id, name, position, weight, kind, hlc)
SELECT pg_temp.uid(), c.school_id, y.id, n, p, w, 'entered', 'load'
FROM yrs y, ctx c, (VALUES ('Test 1', 1, 15), ('Test 2', 2, 15), ('Mid-year', 3, 30), ('Final', 4, 40)) v(n, p, w)
WHERE NOT (y.no = 3 AND v.p = 4);

INSERT INTO assessment_max_marks (id, school_id, assessment_type_id, class_id, subject_id, max_marks, hlc)
SELECT pg_temp.uid(), a.school_id, a.id, cs.class_id, cs.subject_id, CASE WHEN a.position < 3 THEN 20 ELSE 100 END, 'load'
FROM assessment_types a JOIN class_subjects cs ON cs.school_id = a.school_id;

CREATE TEMP TABLE msh AS
SELECT pg_temp.uid() AS id, a.id AS assessment_id, a.position, sec.id AS section_id, sec.class_id, sec.name, cs.subject_id
FROM assessment_types a JOIN sections sec ON sec.academic_year_id = a.academic_year_id
JOIN class_subjects cs ON cs.class_id = sec.class_id;

INSERT INTO mark_sheets (id, school_id, assessment_type_id, section_id, subject_id, status, submitted_by, submitted_at,
                         approved_by, approved_at, hlc)
SELECT m.id, c.school_id, m.assessment_id, m.section_id, m.subject_id, 'approved', c.admin_id, now(), c.admin_id, now(), 'load'
FROM msh m, ctx c;

INSERT INTO marks (id, school_id, sheet_id, student_id, obtained, is_absent, hlc)
SELECT pg_temp.uid(), c.school_id, m.id, st.id,
       round(((CASE WHEN m.position < 3 THEN 20 ELSE 100 END) * (0.45 + ((st.n * 7 + m.position * 13) % 50) / 100.0))::numeric, 2),
       FALSE, 'load'
FROM msh m JOIN stu st ON st.class_id = m.class_id AND st.sec_name = m.name, ctx c;

COMMIT;
-- What autovacuum leaves behind in a database that has been in use for three years, done at
-- once: without it the first screens would pay for it.
VACUUM (ANALYZE);

SELECT 'students' AS what, count(*) FROM students
UNION ALL SELECT 'guardians', count(*) FROM guardians
UNION ALL SELECT 'staff', count(*) FROM staff
UNION ALL SELECT 'sections (3 years)', count(*) FROM sections
UNION ALL SELECT 'fee vouchers', count(*) FROM fee_vouchers
UNION ALL SELECT 'fee receipts', count(*) FROM fee_receipts
UNION ALL SELECT 'attendance sheets', count(*) FROM attendance_sheets
UNION ALL SELECT 'attendance entries', count(*) FROM attendance_entries
UNION ALL SELECT 'marks', count(*) FROM marks;
