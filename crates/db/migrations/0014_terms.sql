-- M2 stage 5: academic year → terms → assessment components. Existing assessment types become
-- entered components of one term per year, so every result stays identical.

CREATE TABLE terms (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    name              TEXT NOT NULL,
    position          INTEGER NOT NULL DEFAULT 0,
    starts_on         DATE,
    ends_on           DATE,
    weight            NUMERIC(5,2) NOT NULL DEFAULT 0 CHECK (weight >= 0 AND weight <= 100),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX terms_name_uq ON terms (academic_year_id, lower(name)) WHERE deleted_at IS NULL;

ALTER TABLE assessment_types
    ADD COLUMN term_id UUID REFERENCES terms (id),
    ADD COLUMN kind TEXT NOT NULL DEFAULT 'entered' CHECK (kind IN ('entered', 'calculated')),
    -- Calculated components: { "op": "sum|average|weighted|best_of", "inputs": [{ "assessment_type_id", "weight"? }], "n"? }
    ADD COLUMN formula JSONB;

-- One term per year that already has assessments; weights move unchanged.
INSERT INTO terms (id, school_id, academic_year_id, name, position, weight, hlc)
SELECT gen_random_uuid(), a.school_id, a.academic_year_id, 'Year', 0, 100, '0000000000000-00000-000000000000'
FROM (SELECT DISTINCT school_id, academic_year_id FROM assessment_types WHERE deleted_at IS NULL) a;

UPDATE assessment_types at SET term_id = t.id
FROM terms t
WHERE t.academic_year_id = at.academic_year_id AND t.deleted_at IS NULL AND at.term_id IS NULL;
