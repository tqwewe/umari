import {
  defineCommand,
  defineEvent,
  defineFold,
  exportCommand,
} from "@umari/js";

type UserRegisteredData = {
  userId: bigint;
  email: string;
};

export const UserRegistered = defineEvent<UserRegisteredData>()("user.registered", {
  domainIds: ["userId"],
});

const UserExistsFold = defineFold({
  domainIds: ["userId"] as const,
  events: [UserRegistered],
  initial: () => false,
  apply: (_state, _event) => true,
});

type Input = {
  userId: bigint;
  email: string;
};

const RegisterUser = defineCommand<Input, { exists: ReturnType<typeof UserExistsFold> }>({
  domainIds: ["userId"] as const,
  folds: ({ userId }) => ({
    exists: UserExistsFold({ userId }),
  }),
  execute: ({ input, folds, emit }) => {
    if (folds.exists) return emit(); // idempotent — already registered
    return emit(
      UserRegistered({
        userId: input.userId,
        email: input.email,
      }),
    );
  },
});

export const { schema, execute } = exportCommand(RegisterUser);
