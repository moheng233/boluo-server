UPDATE messages
SET deleted = true
WHERE id = $1
RETURNING messages;
