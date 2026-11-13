# 0007 — Restoring a backup while joined: head office starts its copy afresh
Date: 2026-11-13
Status: accepted

## Context

A backup holds everything in a school's database, including its link to the Nexus hub and
its record of what it had already sent (ADR 0005). Restoring an older backup while joined
put the school back to that day: its "sent" record said everything was sent, so the card
showed the school in step, while head office's copy still held every record made after the
backup. The restored school then issued the same student numbers again, the hub refused the
new records as clashes and kept them waiting ("parked") for ever.

The two copies can only be made equal again by one side taking the other's data. The school
is the authority for its own records, and a school restores a backup because that is the
state it wants to be in.

## Decision

1. **Restore keeps the present link.** While joined, the hub link and key as they are just
   before the restore win over the backup's. An install that is not joined but restores a
   backup that was made while joined takes the backup's link. Restoring another school's
   backup while joined is refused.
2. **The restore asks first.** While joined the restore needs an explicit confirmation
   (`confirm_network`), with a fixed warning that head office will be asked to reset its copy.
3. **Everything is queued again.** The restore clears what was waiting to be sent or
   applied, switches change capture on afresh, queues every record, and marks the link
   *restored, head office not told yet*.
4. **Head office is told before anything is sent.** At the next sync the school calls
   `POST /sync/restored` with its school record. The hub renames the school's schema to an
   archive (`a_<id>`, listed in `hub_school_archives`), creates a fresh schema under the
   school's own name from the school record, exactly as when joining, and logs it
   (`school_restored`). The school logs the answer, then sends its records into the fresh
   copy. Until the hub has answered, nothing is pushed.
5. **The archive is kept, not read.** It holds what head office had, including corrections
   and parents' messages the school had not received. It is dropped when the school is
   purged.
6. **"In step" means in step.** The school's status says in step only when nothing waits to
   be sent, nothing is parked on either side, the last sync worked and no restore waits to be
   told. Records parked at the hub are counted and explained on the card.

## Consequences

- After a restore the two copies agree record for record, and new records cannot clash with
  records only head office had.
- Head office loses, from its live copy, what it changed after the backup was made. That is
  the meaning of restoring a backup; the archive keeps it.
- A hub older than 0.4.3 does not know `/sync/restored`: the school then says so and does not
  sync. The fallback is to leave the network and join again with a new join code, which also
  starts a fresh copy.
- The parent app keeps working through the switch; for a moment, while the fresh copy is being
  filled, a parent may see fewer records.
