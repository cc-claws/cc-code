    #[test]
    fn checkbox_state_toggle() {
        let mut state = CheckboxState::new(3);
        state.toggle();
        assert!(state.is_checked(0));
        state.toggle();
        assert!(!state.is_checked(0));
    }

    #[test]
    fn checkbox_state_select_all_none() {
        let mut state = CheckboxState::new(3);
        state.select_all();
        assert_eq!(state.checked_indices(), vec![0, 1, 2]);
        state.select_none();
        assert_eq!(state.checked_indices(), Vec::<usize>::new());
    }

    #[test]
    fn checkbox_state_move_cursor() {
        let mut state = CheckboxState::new(3);
        state.move_cursor(1);
        assert_eq!(state.cursor(), 1);
        state.move_cursor(100);
        assert_eq!(state.cursor(), 2); // clamped
    }
