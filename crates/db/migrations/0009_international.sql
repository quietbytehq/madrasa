-- Schools outside Pakistan: a country and a time zone on the school, any month as the start
-- of the academic year (US schools start in August or September), and the national identity
-- number and phone formats follow the country (enforced in code, not here).

ALTER TABLE schools ADD COLUMN country   TEXT NOT NULL DEFAULT 'PK';
ALTER TABLE schools ADD COLUMN time_zone TEXT NOT NULL DEFAULT 'Asia/Karachi';

ALTER TABLE schools DROP CONSTRAINT IF EXISTS schools_academic_year_start_month_check;
ALTER TABLE schools ADD CONSTRAINT schools_academic_year_start_month_check
    CHECK (academic_year_start_month BETWEEN 1 AND 12);

ALTER TABLE academic_years DROP CONSTRAINT IF EXISTS academic_years_start_month_check;
ALTER TABLE academic_years ADD CONSTRAINT academic_years_start_month_check
    CHECK (start_month BETWEEN 1 AND 12);
