-- Preserve access and delivery for channels with existing authored teletext.
ALTER TABLE "Channel" ADD COLUMN "TeletextEnabled" INTEGER NOT NULL DEFAULT 0
    CONSTRAINT "CK_Channel_TeletextEnabled" CHECK ("TeletextEnabled" IN (0, 1));

UPDATE "Channel" SET "TeletextEnabled" = 1 WHERE "Id" IN (
    SELECT "ChannelId" FROM "ChannelTeletextBinding"
    UNION SELECT "ChannelId" FROM "ChannelTeletextAtlas"
    UNION SELECT "ChannelId" FROM "TeletextPublicationRequestCurrent"
    UNION SELECT "ChannelId" FROM "TeletextEffectivePublicationRequestCurrent"
    UNION SELECT "ChannelId" FROM "TeletextAcceptedGenerationCurrent"
    UNION SELECT "ChannelId" FROM "TeletextEffectiveAcceptedGenerationCurrent"
    UNION SELECT p."ChannelId" FROM "Playout" p
        JOIN "PlayoutTeletextInterval" i ON i."PlayoutId" = p."Id"
);
