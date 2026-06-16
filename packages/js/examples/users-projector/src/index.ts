import {
  defineEvent,
  defineProjector,
  exportProjector,
  sqlite,
} from "@umari/js";

type UserRegisteredData = {
  userId: bigint;
  email: string;
};

const UserRegistered = defineEvent<UserRegisteredData>()("user.registered", {
  domainIds: ["userId"],
});

const Users = defineProjector({
  events: [UserRegistered],
  init: () => {
    sqlite.executeBatch(
      `CREATE TABLE IF NOT EXISTS users (
         user_id TEXT PRIMARY KEY,
         email TEXT NOT NULL
       );`,
    );
  },
  handle: (event) => {
    switch (event.type) {
      case "user.registered":
        sqlite.execute(
          "INSERT INTO users (user_id, email) VALUES (?, ?)",
          [event.data.userId, event.data.email],
        );
        break;
    }
  },
});

export const { projector } = exportProjector(Users);
